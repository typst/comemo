use super::*;

/// Make a type trackable.
pub fn expand(block: syn::ItemImpl) -> Result<proc_macro2::TokenStream> {
    let ty = &block.self_ty;

    let mut tracked_methods = vec![];
    for item in &block.items {
        let method = match item {
            syn::ImplItem::Method(method) => method,
            _ => bail!(item, "only methods are supported"),
        };

        let mut tracked = method.clone();
        let name = &tracked.sig.ident;

        let mut inputs = tracked.sig.inputs.iter();
        let receiver = match inputs.next() {
            Some(syn::FnArg::Receiver(recv)) => recv,
            _ => bail!(tracked, "method must take self"),
        };

        if receiver.reference.is_none() || receiver.mutability.is_some() {
            bail!(receiver, "must take self by shared reference");
        }

        let mut args = vec![];
        for input in inputs {
            let pat = match input {
                syn::FnArg::Typed(typed) => &*typed.pat,
                syn::FnArg::Receiver(_) => unreachable!("unexpected second self"),
            };

            let ident = match pat {
                syn::Pat::Ident(ident) => ident,
                _ => bail!(pat, "only simple identifiers are supported"),
            };

            args.push(ident);
        }

        tracked.block = parse_quote! { { self.0.#name(#(#args),*) } };
        tracked_methods.push(tracked);
    }

    let track_impl = quote! {
        impl comemo::Track for #ty {
            type Surface = Surface;

            fn surface(&self) -> &Surface {
                unsafe { &*(self as *const Self as *const Surface) }
            }
        }

        #[repr(transparent)]
        struct Surface(#ty);

        impl Surface {
            #(#tracked_methods)*
        }
    };

    Ok(quote! {
        #block
        const _: () = { #track_impl };
    })
}
