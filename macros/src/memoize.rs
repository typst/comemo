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

        let name = match typed.pat.as_ref() {
            syn::Pat::Ident(syn::PatIdent {
                by_ref: None,
                mutability: None,
                ident,
                subpat: None,
                ..
            }) => ident,
            pat => bail!(pat, "only simple identifiers are supported"),
        };

        let ty = typed.ty.as_ref();
        args.push(name);
        types.push(ty);
    }

    // Construct a tuple from all arguments.
    let arg_tuple = quote! { (#(#args,)*) };

    // Construct assertions that the arguments fulfill the necessary bounds.
    let bounds = types.iter().map(|ty| {
        quote! {
            ::comemo::internal::assert_hashable_or_trackable::<#ty>();
        }
    });

    // Construct the inner closure.
    let body = &func.block;
    let closure = quote! { |#arg_tuple| #body };

    // Adjust the function's body.
    let name = func.sig.ident.to_string();
    func.block = parse_quote! { {
        #(#bounds;)*
        ::comemo::internal::cached(
            #name,
            ::comemo::internal::Args(#arg_tuple),
            #closure,
        )
    } };

    Ok(quote! { #func })
}
