use super::*;

/// Make a type trackable.
pub fn expand(mut func: syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    let name = func.sig.ident.to_string();

    let mut args = vec![];
    let mut asserts = vec![];
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

        asserts.push(quote_spanned! { ident.span() => assert_hash(&#ident); });
        args.push(ident);
    }

    func.block.stmts.insert(0, parse_quote! { {
        println!("calling {}", #name);
        fn assert_hash<T: std::hash::Hash>(_: &T) {}
        // #(#asserts)*
    } });

    Ok(quote! { #func })
}
