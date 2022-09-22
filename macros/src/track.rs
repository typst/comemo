use super::*;

/// Make a type trackable.
pub fn expand(item: &syn::Item) -> Result<TokenStream> {
    // Preprocess and validate the methods.
    let mut methods = vec![];

    let (ty, trait_) = match item {
        syn::Item::Impl(item) => {
            for param in item.generics.params.iter() {
                bail!(param, "tracked impl blocks cannot be generic")
            }

            for item in &item.items {
                methods.push(prepare_impl_method(&item)?);
            }

            let ty = item.self_ty.as_ref().clone();
            (ty, None)
        }
        syn::Item::Trait(item) => {
            for item in &item.items {
                methods.push(prepare_trait_method(&item)?);
            }

            let name = &item.ident;
            let ty = parse_quote! { dyn #name };
            (ty, Some(name.clone()))
        }
        _ => bail!(
            item,
            "`track` can only be applied to impl blocks and traits"
        ),
    };

    // Produce the necessary items for the type to become trackable.
    let scope = create(&ty, trait_, &methods)?;

    Ok(quote! {
        #item
        const _: () = { mod private { #scope } };
    })
}

/// Details about a method that should be tracked.
struct Method {
    vis: syn::Visibility,
    sig: syn::Signature,
    args: Vec<syn::Ident>,
    types: Vec<syn::Type>,
    kinds: Vec<Kind>,
}

/// Whether an argument to a tracked method is bare or by reference.
enum Kind {
    Normal,
    Reference,
}

/// Preprocess and validate a method in an impl block.
fn prepare_impl_method(item: &syn::ImplItem) -> Result<Method> {
    let method = match item {
        syn::ImplItem::Method(method) => method,
        _ => bail!(item, "only methods can be tracked"),
    };

    let vis = match method.vis {
        syn::Visibility::Inherited => parse_quote! { pub(super) },
        syn::Visibility::Public(_) => parse_quote! { pub },
        _ => bail!(method.vis, "only private and public methods can be tracked"),
    };

    prepare_method(vis, &method.sig)
}

/// Preprocess and validate a method in a trait.
fn prepare_trait_method(item: &syn::TraitItem) -> Result<Method> {
    let method = match item {
        syn::TraitItem::Method(method) => method,
        _ => bail!(item, "only methods can be tracked"),
    };

    prepare_method(syn::Visibility::Inherited, &method.sig)
}

/// Preprocess and validate a method signature.
fn prepare_method(vis: syn::Visibility, sig: &syn::Signature) -> Result<Method> {
    if let Some(unsafety) = sig.unsafety {
        bail!(unsafety, "unsafe methods cannot be tracked");
    }

    if let Some(asyncness) = sig.asyncness {
        bail!(asyncness, "async methods cannot be tracked");
    }

    if let Some(constness) = sig.constness {
        bail!(constness, "const methods cannot be tracked");
    }

    for param in sig.generics.params.iter() {
        match param {
            syn::GenericParam::Const(_) | syn::GenericParam::Type(_) => {
                bail!(param, "tracked method cannot be generic")
            }
            syn::GenericParam::Lifetime(_) => {}
        }
    }

    let mut inputs = sig.inputs.iter();
    let receiver = match inputs.next() {
        Some(syn::FnArg::Receiver(recv)) => recv,
        _ => bail!(sig, "tracked method must take self"),
    };

    if receiver.reference.is_none() || receiver.mutability.is_some() {
        bail!(
            receiver,
            "tracked method must take self by shared reference"
        );
    }

    let mut args = vec![];
    let mut types = vec![];
    let mut kinds = vec![];

    for input in inputs {
        let typed = match input {
            syn::FnArg::Typed(typed) => typed,
            syn::FnArg::Receiver(_) => continue,
        };

        let name = match typed.pat.as_ref() {
            syn::Pat::Ident(syn::PatIdent {
                by_ref: None,
                mutability: None,
                ident,
                subpat: None,
                ..
            }) => ident.clone(),
            pat => bail!(pat, "only simple identifiers are supported"),
        };

        let (ty, kind) = match typed.ty.as_ref() {
            syn::Type::ImplTrait(ty) => {
                bail!(ty, "tracked methods cannot be generic");
            }
            syn::Type::Reference(syn::TypeReference { mutability, elem, .. }) => {
                if mutability.is_some() {
                    bail!(typed.ty, "tracked methods cannot have mutable parameters");
                } else {
                    (elem.as_ref().clone(), Kind::Reference)
                }
            }
            ty => (ty.clone(), Kind::Normal),
        };

        args.push(name);
        types.push(ty);
        kinds.push(kind)
    }

    match sig.output {
        syn::ReturnType::Default => {
            bail!(sig, "tracked methods must have a return type")
        }
        syn::ReturnType::Type(..) => {}
    }

    Ok(Method {
        vis,
        sig: sig.clone(),
        args,
        types,
        kinds,
    })
}

/// Produce the necessary items for a type to become trackable.
fn create(
    ty: &syn::Type,
    trait_: Option<syn::Ident>,
    methods: &[Method],
) -> Result<TokenStream> {
    let surface = quote! { __ComemoSurface };
    let family = quote! { __ComemoSurfaceFamily };
    let constraint = quote! { __ComemoConstraint };

    let validations = methods.iter().map(create_validation);
    let wrapper_methods = methods.iter().map(create_wrapper);
    let constraint_fields = methods.iter().map(create_constraint);
    let join_calls = methods.iter().map(create_join);
    let prefix = trait_.map(|name| quote! { #name for });

    Ok(quote! {
        use super::*;

        #[doc(hidden)]
        impl ::comemo::Track for #ty {}

        #[doc(hidden)]
        impl ::comemo::internal::Trackable for #ty {
            type Constraint = #constraint;
            type Surface = #family;

            fn valid(&self, constraint: &Self::Constraint) -> bool {
                true #(&& #validations)*
            }

            fn surface<'a, 'r>(
                tracked: &'r ::comemo::Tracked<'a, #ty>,
            ) -> &'r #surface<'a> {
                // Safety: Surface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }
        }

        pub enum #family {}
        impl<'a> ::comemo::internal::Family<'a> for #family {
            type Out = #surface<'a>;
        }

        #[repr(transparent)]
        pub struct #surface<'a>(::comemo::Tracked<'a, #ty>);

        impl #prefix #surface<'_> {
            #(#wrapper_methods)*
        }

        #[derive(Debug, Default)]
        pub struct #constraint {
            #(#constraint_fields)*
        }

        impl ::comemo::internal::Join for #constraint {
            fn join(&self, inner: &Self) {
                #(#join_calls)*
            }
        }
    })
}

/// Produce a constraint validation for a method.
fn create_validation(method: &Method) -> TokenStream {
    let name = &method.sig.ident;
    let args = &method.args;
    let prepared = method.args.iter().zip(&method.kinds).map(|(arg, kind)| match kind {
        Kind::Normal => quote! { #arg.to_owned() },
        Kind::Reference => quote! { #arg },
    });
    quote! {
        constraint.#name
            .valid(|(#(#args,)*)| ::comemo::internal::hash(&self.#name(#(#prepared,)*)))
    }
}

/// Produce a wrapped surface method.
fn create_wrapper(method: &Method) -> TokenStream {
    let vis = &method.vis;
    let sig = &method.sig;
    let name = &method.sig.ident;
    let args = &method.args;
    quote! {
        #[track_caller]
        #vis #sig {
            let input = (#(#args.to_owned(),)*);
            let (value, constraint) = ::comemo::internal::to_parts(self.0);
            let output = value.#name(#(#args,)*);
            if let Some(constraint) = &constraint {
                constraint.#name.set(input, ::comemo::internal::hash(&output));
            }
            output
        }
    }
}

/// Produce a constraint field for a method.
fn create_constraint(method: &Method) -> TokenStream {
    let name = &method.sig.ident;
    let types = &method.types;
    if types.is_empty() {
        quote! { #name: ::comemo::internal::SoloConstraint, }
    } else {
        quote_spanned! { method.sig.span() =>
            #name: ::comemo::internal::MultiConstraint<
                (#(<#types as ::std::borrow::ToOwned>::Owned,)*)
            >,
        }
    }
}

/// Produce a join call for a method's constraint.
fn create_join(method: &Method) -> TokenStream {
    let name = &method.sig.ident;
    quote! { self.#name.join(&inner.#name); }
}
