use super::*;

/// Make a type trackable.
pub fn expand(func: &syn::ItemFn) -> Result<proc_macro2::TokenStream> {
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

    let cts = args.iter().map(|arg| {
        quote! {
            Validate::constraint(&#arg)
        }
    });

    let mut outer = func.clone();
    outer.block = parse_quote! { { #inner {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use comemo::Validate;

        static NR: AtomicUsize = AtomicUsize::new(1);
        let nr = NR.fetch_add(1, Ordering::SeqCst);
        let cts = (#(#cts,)*);

        println!("{:?}", cts);

        let mut hit = false;
        let result = inner(#(#args),*);

        println!(
            "{} {} {} {}",
            #name,
            nr,
            if hit { "[hit]: " } else { "[miss]:" },
            result,
        );

        result
    } } };

    Ok(quote! { #outer })
}
