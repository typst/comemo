use super::*;

/// Make a type trackable.
pub fn expand(block: &syn::ItemImpl) -> Result<proc_macro2::TokenStream> {
    let ty = &block.self_ty;

    // Extract and validate the methods.
    let mut methods = vec![];
    for item in &block.items {
        methods.push(method(&item)?);
    }

    let tracked_fields = methods.iter().map(|method| {
        let name = &method.sig.ident;
        let ty = match &method.sig.output {
            syn::ReturnType::Default => unreachable!(),
            syn::ReturnType::Type(_, ty) => ty.as_ref(),
        };
        quote! { #name: ::comemo::internal::HashConstraint<#ty>, }
    });

    let tracked_methods = methods.iter().map(|method| {
        let name = &method.sig.ident;
        let mut method = (*method).clone();
        if matches!(method.vis, syn::Visibility::Inherited) {
            method.vis = parse_quote! { pub(super) };
        }
        method.block = parse_quote! { {
            let (inner, constraint) = ::comemo::internal::to_parts(self.0);
            let output = inner.#name();
            if let Some(constraint) = &constraint {
                constraint.#name.set(&output);
            }
            output
        } };
        method
    });

    let tracked_valids = methods.iter().map(|method| {
        let name = &method.sig.ident;
        quote! {
            constraint.#name.valid(&self.#name())
        }
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
    };

    Ok(quote! {
        #block
        const _: () = { mod private { #track_impl } };
    })
}

/// Extract and validate a method.
fn method(item: &syn::ImplItem) -> Result<&syn::ImplItemMethod> {
    let method = match item {
        syn::ImplItem::Method(method) => method,
        _ => bail!(item, "only methods are supported"),
    };

    match method.vis {
        syn::Visibility::Inherited => {}
        syn::Visibility::Public(_) => {}
        _ => bail!(method.vis, "only private and public methods are supported"),
    }

    let mut inputs = method.sig.inputs.iter();
    let receiver = match inputs.next() {
        Some(syn::FnArg::Receiver(recv)) => recv,
        _ => bail!(method, "method must take self"),
    };

    if receiver.reference.is_none() || receiver.mutability.is_some() {
        bail!(receiver, "must take self by shared reference");
    }

    if inputs.next().is_some() {
        bail!(
            method.sig,
            "currently, only methods without extra arguments are supported"
        );
    }

    let output = &method.sig.output;
    match output {
        syn::ReturnType::Default => {
            bail!(method.sig, "method must have a return type")
        }
        syn::ReturnType::Type(..) => {}
    }

    Ok(method)
}
