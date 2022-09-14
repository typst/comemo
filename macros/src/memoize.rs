use super::*;

/// Memoize a function.
pub fn expand(func: &syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    let mut args = vec![];
    let mut types = vec![];
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
        types.push(typed.ty.as_ref());
    }

    let mut inner = func.clone();
    let arg_tuple = quote! { (#(#args,)*) };
    let type_tuple = quote! { (#(#types,)*) };
    inner.sig.inputs = parse_quote! { #arg_tuple: #type_tuple };

    let bounds = args.iter().zip(&types).map(|(arg, ty)| {
        quote_spanned! {
            arg.span() => ::comemo::internal::assert_hashable_or_trackable::<#ty>();
        }
    });

    let mut outer = func.clone();
    let name = &func.sig.ident;
    outer.block = parse_quote! { {
        #inner
        #(#bounds;)*
        ::comemo::internal::CACHE.with(|cache|
            cache.query(
                stringify!(#name),
                ::comemo::internal::Args(#arg_tuple),
                #name,
            )
        )
    } };

    Ok(quote! { #outer })
}
