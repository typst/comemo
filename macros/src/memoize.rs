use super::*;

/// Memoize a function.
pub fn expand(mut func: syn::ItemFn) -> Result<proc_macro2::TokenStream> {
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

    // Construct a tuple from all arguments.
    let arg_tuple = quote! { (#(#args,)*) };

    // Construct assertions that the arguments fulfill the necessary bounds.
    let bounds = args.iter().zip(&types).map(|(arg, ty)| {
        quote_spanned! {
            arg.span() => ::comemo::internal::assert_hashable_or_trackable::<#ty>();
        }
    });

    // Construct the inner closure.
    let body = &func.block;
    let inner = quote! { |#arg_tuple| #body };

    // Adjust the function's body.
    let name = func.sig.ident.to_string();
    func.block = parse_quote! { {
        #(#bounds;)*
        ::comemo::internal::cached(
            #name,
            ::comemo::internal::Args(#arg_tuple),
            #inner,
        )
    } };

    Ok(quote! { #func })
}
