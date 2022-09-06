use super::*;

/// Make a type trackable.
pub fn expand(mut func: syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    let name = func.sig.ident.to_string();

    let mut args = vec![];
    for input in &func.sig.inputs {
        let typed = match input {
            syn::FnArg::Typed(typed) => typed,
            syn::FnArg::Receiver(_) => {
                bail!(input, "methods are not supported")
            }
        };

        let ident = match &*typed.pat {
            syn::Pat::Ident(ident) => ident,
            _ => bail!(typed.pat, "only simple identifiers are supported"),
        };

        args.push(ident);
    }

    let mut inner = func.clone();
    inner.sig.ident = syn::Ident::new("inner", Span::call_site());

    let asserts = args.iter().map(|ident| {
        quote_spanned! {
            ident.span() =>
            argument_must_be_hash_or_tracked(&#ident);
        }
    });

    let hashes = args.iter().map(|ident| {
        quote! {
            #ident.hash(&mut state);
        }
    });

    func.block = parse_quote! { { #inner {
        fn argument_must_be_hash_or_tracked<T: std::hash::Hash>(_: &T) {}
        #(#asserts)*

        use std::collections::HashMap;
        use std::hash::Hash;
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Mutex;
        use comemo::once_cell::sync::Lazy;
        use comemo::siphasher::sip128::{Hasher128, SipHasher};

        static NR: AtomicUsize = AtomicUsize::new(1);
        static MAP: Lazy<Mutex<HashMap<u128, String>>> =
            Lazy::new(|| Mutex::new(HashMap::new()));

        let nr = NR.fetch_add(1, Ordering::SeqCst);
        let hash = {
            let mut state = SipHasher::new();
            #(#hashes)*
            state.finish128().as_u128()
        };

        let mut hit = true;
        let result = MAP
            .lock()
            .unwrap()
            .entry(hash)
            .or_insert_with(|| {
                hit = false;
                inner(#(#args),*)
            })
            .clone();

        println!(
            "{} {} {} {}",
            #name,
            nr,
            if hit { "[hit]: " } else { "[miss]:" },
            result,
        );

        result
    } } };

    Ok(quote! { #func })
}
