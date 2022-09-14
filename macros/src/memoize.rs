use super::*;

/// Memoize a function.
pub fn expand(func: &syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    let mut args = vec![];
    for input in &func.sig.inputs {
        let typed = match input {
            syn::FnArg::Typed(typed) => typed,
            syn::FnArg::Receiver(_) => {
                bail!(input, "methods are not supported")
            }
        };

        let name = match &*typed.pat {
            syn::Pat::Ident(ident) => ident,
            _ => bail!(typed.pat, "only simple identifiers are supported"),
        };

        args.push(name);
    }

    let mut outer = func.clone();
    let name = &func.sig.ident;
    let arg = &args[0];

    outer.block = parse_quote! { {
        #func
        ::comemo::internal::CACHE.with(|cache|
            cache.query(
                stringify!(#name),
                #arg,
                #name,
            )
        )
    } };

    Ok(quote! { #outer })
}
