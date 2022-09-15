use super::*;

/// Make a type trackable.
pub fn expand(block: syn::ItemImpl) -> Result<proc_macro2::TokenStream> {
    let ty = &block.self_ty;

    // Extract and validate the methods.
    let mut methods = vec![];
    for item in &block.items {
        methods.push(method(&item)?);
    }

    let tracked_valids = methods.iter().map(|method| {
        let name = &method.name;
        let args = &method.args;
        if args.is_empty() {
            quote! { constraint.#name.valid(&self.#name()) }
        } else {
            quote! {
                constraint.#name
                    .valid(|(#(#args,)*)| self.#name(#(#args.clone(),)*))
            }
        }
    });

    let tracked_methods = methods.iter().map(|method| {
        let mut wrapper = method.item.clone();
        if matches!(wrapper.vis, syn::Visibility::Inherited) {
            wrapper.vis = parse_quote! { pub(super) };
        }

        let name = &method.name;
        let args = &method.args;
        let set = if args.is_empty() {
            quote! { constraint.#name.set(&output) }
        } else {
            quote! { constraint.#name.set((#(#args,)*), &output) }
        };

        // Construct assertions that the arguments fulfill the necessary bounds.
        let bounds = method.types.iter().map(|ty| {
            quote! {
                ::comemo::internal::assert_clone_and_partial_eq::<#ty>();
            }
        });

        wrapper.block = parse_quote! { {
            #(#bounds;)*
            let (value, constraint) = ::comemo::internal::to_parts(self.0);
            let output = value.#name(#(#args.clone(),)*);
            if let Some(constraint) = &constraint {
                #set;
            }
            output
        } };

        wrapper
    });

    let tracked_fields = methods.iter().map(|method| {
        let name = &method.name;
        let types = &method.types;
        if types.is_empty() {
            quote! { #name: ::comemo::internal::HashConstraint, }
        } else {
            quote! { #name: ::comemo::internal::FuncConstraint<(#(#types,)*)>, }
        }
    });

    let join_calls = methods.iter().map(|method| {
        let name = &method.name;
        quote! { self.#name.join(&inner.#name); }
    });

    let track_impl = quote! {
        use super::*;

        impl ::comemo::Track for #ty {}
        impl ::comemo::internal::Trackable for #ty {
            type Constraint = Constraint;
            type Surface = SurfaceFamily;

            fn valid(&self, constraint: &Self::Constraint) -> bool {
                true #(&& #tracked_valids)*
            }

            fn surface<'a, 'r>(tracked: &'r Tracked<'a, #ty>) -> &'r Surface<'a> {
                // Safety: Surface is repr(transparent).
                unsafe { &*(tracked as *const _ as *const _) }
            }
        }

        pub enum SurfaceFamily {}
        impl<'a> ::comemo::internal::Family<'a> for SurfaceFamily {
            type Out = Surface<'a>;
        }

        #[repr(transparent)]
        pub struct Surface<'a>(Tracked<'a, #ty>);

        impl Surface<'_> {
            #(#tracked_methods)*
        }

        #[derive(Default)]
        pub struct Constraint {
            #(#tracked_fields)*
        }

        impl ::comemo::internal::Join for Constraint {
            fn join(&self, inner: &Self) {
                #(#join_calls)*
            }
        }
    };

    Ok(quote! {
        #block
        const _: () = { mod private { #track_impl } };
    })
}

struct Method {
    item: syn::ImplItemMethod,
    name: syn::Ident,
    args: Vec<syn::Ident>,
    types: Vec<syn::Type>,
}

/// Extract and validate a method.
fn method(item: &syn::ImplItem) -> Result<Method> {
    let method = match item {
        syn::ImplItem::Method(method) => method,
        _ => bail!(item, "only methods are supported"),
    };

    match method.vis {
        syn::Visibility::Inherited => {}
        syn::Visibility::Public(_) => {}
        _ => bail!(method.vis, "only private and public methods are supported"),
    }

    if let Some(unsafety) = method.sig.unsafety {
        bail!(unsafety, "unsafe methods are not supported");
    }

    if let Some(asyncness) = method.sig.asyncness {
        bail!(asyncness, "async methods are not supported");
    }

    if let Some(constness) = method.sig.constness {
        bail!(constness, "const methods are not supported");
    }

    for param in method.sig.generics.params.iter() {
        match param {
            syn::GenericParam::Const(_) | syn::GenericParam::Type(_) => {
                bail!(param, "method must not be generic")
            }
            syn::GenericParam::Lifetime(_) => {}
        }
    }

    let mut inputs = method.sig.inputs.iter();
    let receiver = match inputs.next() {
        Some(syn::FnArg::Receiver(recv)) => recv,
        _ => bail!(method, "method must take self"),
    };

    if receiver.reference.is_none() || receiver.mutability.is_some() {
        bail!(receiver, "must take self by shared reference");
    }

    let mut args = vec![];
    let mut types = vec![];
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

        let ty = (*typed.ty).clone();
        match ty {
            syn::Type::ImplTrait(_) => bail!(ty, "method must not be generic"),
            _ => {}
        }

        args.push(name);
        types.push(ty);
    }

    match method.sig.output {
        syn::ReturnType::Default => {
            bail!(method.sig, "method must have a return type")
        }
        syn::ReturnType::Type(..) => {}
    }

    Ok(Method {
        item: method.clone(),
        name: method.sig.ident.clone(),
        args,
        types,
    })
}
