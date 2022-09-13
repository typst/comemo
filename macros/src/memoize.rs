use super::*;

/// Memoize a function.
pub fn expand(func: &syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    let name = &func.sig.ident;

    let mut args = vec![];
    let mut types = vec![];
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
        types.push(typed.ty.as_ref());
    }

    if args.len() != 1 {
        bail!(func, "expected exactly one argument");
    }

    let arg = args[0];
    let ty = types[0];
    let _inner = match ty {
        syn::Type::Path(path) => {
            let segs = &path.path.segments;
            if segs.len() != 1 {
                bail!(ty, "expected exactly one path segment")
            }
            let args = match &segs[0].arguments {
                syn::PathArguments::AngleBracketed(args) => &args.args,
                _ => bail!(ty, "expected `Tracked<_>` type"),
            };
            if args.len() != 1 {
                bail!(args, "expected exactly one generic argument")
            }
            match &args[0] {
                syn::GenericArgument::Type(ty) => ty,
                ty => bail!(ty, "expected type argument"),
            }
        }
        _ => bail!(ty, "expected type of the form `Tracked<_>`"),
    };

    let mut outer = func.clone();
    outer.block = parse_quote! { {
        #func
        ::comemo::internal::CACHE.with(|cache|
            cache.query(stringify!(#name), #name, #arg)
        )
    } };

    Ok(quote! { #outer })
}
