use super::*;

/// Make a type trackable.
pub fn expand(block: &syn::ItemImpl) -> Result<proc_macro2::TokenStream> {
    let ty = &block.self_ty;

    // Extract and validate the methods.
    let mut methods = vec![];
    for item in &block.items {
        methods.push(method(&item)?);
    }

    let tracked_methods = methods.iter().map(|method| {
        let mut method = (*method).clone();
        let name = &method.sig.ident;
        method.block = parse_quote! { {
            let output = self.0.#name();
            let slot = &mut self.constraint().#name;
            let ct = Validate::constraint(&output);
            if slot.is_none() {
                assert_eq!(*slot, Some(ct), "comemo: method is not pure");
            }
            *slot = Some(ct);
            output
        } };
        method
    });

    let tracked_fields = methods.iter().map(|method| {
        let name = &method.sig.ident;
        let ty = match &method.sig.output {
            syn::ReturnType::Default => unreachable!(),
            syn::ReturnType::Type(_, ty) => ty.as_ref(),
        };
        quote! { #name: Option<<#ty as Validate>::Constraint>, }
    });

    let track_impl = quote! {
        use comemo::Validate;

        struct Surface<'a>(&'a #ty);

        impl Surface<'_> {
            #(#tracked_methods)*

            fn constraint(&self) -> &mut Constraint {
                todo!()
            }
        }

        impl<'a> From<&'a #ty> for Surface<'a> {
            fn from(val: &'a #ty) -> Self {
                Self(val)
            }
        }

        #[derive(Debug, Default)]
        struct Constraint {
            #(#tracked_fields)*
        }

        impl<'a> comemo::Track<'a> for #ty {
            type Surface = Surface<'a>;
            type Constraint = Constraint;
        }
    };

    Ok(quote! {
        #block
        const _: () = { #track_impl };
    })
}

/// Extract and validate a method.
fn method(item: &syn::ImplItem) -> Result<&syn::ImplItemMethod> {
    let method = match item {
        syn::ImplItem::Method(method) => method,
        _ => bail!(item, "only methods are supported"),
    };

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
