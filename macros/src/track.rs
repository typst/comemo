use super::*;

/// Make a type trackable.
pub fn expand(block: &syn::ItemImpl) -> Result<TokenStream> {
    let ty = &block.self_ty;

    // Preprocess and validate the methods.
    let mut methods = vec![];
    for item in &block.items {
        methods.push(prepare(&item)?);
    }

    // Produce the necessary items for the type to become trackable.
    let scope = process(ty, &methods)?;

    Ok(quote! {
        #block
        const _: () = { mod private { #scope } };
    })
}

/// Details about a method that should be tracked.
struct Method {
    item: syn::ImplItemMethod,
    name: syn::Ident,
    args: Vec<syn::Ident>,
    types: Vec<syn::Type>,
    kinds: Vec<Kind>,
}

/// Whether an argument to a tracked method is bare or by reference.
enum Kind {
    Normal,
    Reference,
}

/// Preprocess and validate a method.
fn prepare(item: &syn::ImplItem) -> Result<Method> {
    let method = match item {
        syn::ImplItem::Method(method) => method,
        _ => bail!(item, "only methods can be tracked"),
    };

    match method.vis {
        syn::Visibility::Inherited => {}
        syn::Visibility::Public(_) => {}
        _ => bail!(method.vis, "only private and public methods can be tracked"),
    }

    if let Some(unsafety) = method.sig.unsafety {
        bail!(unsafety, "unsafe methods cannot be tracked");
    }

    if let Some(asyncness) = method.sig.asyncness {
        bail!(asyncness, "async methods cannot be tracked");
    }

    if let Some(constness) = method.sig.constness {
        bail!(constness, "const methods cannot be tracked");
    }

    for param in method.sig.generics.params.iter() {
        match param {
            syn::GenericParam::Const(_) | syn::GenericParam::Type(_) => {
                bail!(param, "tracked method must not be generic")
            }
            syn::GenericParam::Lifetime(_) => {}
        }
    }

    let mut inputs = method.sig.inputs.iter();
    let receiver = match inputs.next() {
        Some(syn::FnArg::Receiver(recv)) => recv,
        _ => bail!(method, "tracked method must take self"),
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
            syn::Type::ImplTrait(ty) => bail!(ty, "tracked methods must not be generic"),
            syn::Type::Reference(syn::TypeReference { mutability, elem, .. }) => {
                match mutability {
                    None => (elem.as_ref().clone(), Kind::Reference),
                    Some(_) => {
                        bail!(typed.ty, "tracked methods cannot have mutable parameters")
                    }
                }
            }
            ty => (ty.clone(), Kind::Normal),
        };

        args.push(name);
        types.push(ty);
        kinds.push(kind)
    }

    match method.sig.output {
        syn::ReturnType::Default => {
            bail!(method.sig, "tracked methods must have a return type")
        }
        syn::ReturnType::Type(..) => {}
    }

    Ok(Method {
        item: method.clone(),
        name: method.sig.ident.clone(),
        args,
        types,
        kinds,
    })
}

/// Produce the necessary items for a type to become trackable.
fn process(ty: &syn::Type, methods: &[Method]) -> Result<TokenStream> {
    let surface = quote! { __ComemoSurface };
    let family = quote! { __ComemoSurfaceFamily };
    let constraint = quote! { __ComemoConstraint };

    let validations = methods.iter().map(validation);
    let wrapper_methods = methods.iter().map(wrapper_method);
    let constraint_fields = methods.iter().map(constraint_field);
    let join_calls = methods.iter().map(join_call);

    Ok(quote! {
        use super::*;

        impl ::comemo::Track for #ty {}
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

        impl #surface<'_> {
            #(#wrapper_methods)*
        }

        #[derive(Default)]
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
fn validation(method: &Method) -> TokenStream {
    let name = &method.name;
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
fn wrapper_method(method: &Method) -> TokenStream {
    let mut wrapper = method.item.clone();
    if matches!(wrapper.vis, syn::Visibility::Inherited) {
        wrapper.vis = parse_quote! { pub(super) };
    }

    let name = &method.name;
    let args = &method.args;

    wrapper.block = parse_quote! { {
        let input = (#(#args.to_owned(),)*);
        let (value, constraint) = ::comemo::internal::to_parts(self.0);
        let output = value.#name(#(#args,)*);
        if let Some(constraint) = &constraint {
            constraint.#name.set(input, ::comemo::internal::hash(&output));
        }
        output
    } };

    quote! { #wrapper }
}

/// Produce a constraint field for a method.
fn constraint_field(method: &Method) -> TokenStream {
    let name = &method.name;
    let types = &method.types;
    if types.is_empty() {
        quote! { #name: ::comemo::internal::SoloConstraint, }
    } else {
        quote_spanned! { method.item.span() =>
            #name: ::comemo::internal::MultiConstraint<
                (#(<#types as ::std::borrow::ToOwned>::Owned,)*)
            >,
        }
    }
}

/// Produce a join call for a method's constraint.
fn join_call(method: &Method) -> TokenStream {
    let name = &method.name;
    quote! { self.#name.join(&inner.#name); }
}
