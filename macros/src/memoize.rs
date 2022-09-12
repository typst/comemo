use super::*;

/// Memoize a function.
pub fn expand(func: &syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    let name = func.sig.ident.to_string();

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

    let ret = match &func.sig.output {
        syn::ReturnType::Default => {
            bail!(func.sig, "function must have a return type")
        }
        syn::ReturnType::Type(.., ty) => ty.as_ref(),
    };

    let mut inner = func.clone();
    inner.sig.ident = syn::Ident::new("inner", Span::call_site());

    if args.len() != 1 {
        bail!(func, "expected exactly one argument");
    }

    let arg = args[0];
    let ty = types[0];
    let track = match ty {
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

    let trackable = quote! {
        <#track as ::comemo::internal::Trackable<'static>>
    };

    let body = quote! {
        type Cache = ::core::cell::RefCell<
            ::std::vec::Vec<(#trackable::Tracker, #ret)>
        >;

        thread_local! {
            static CACHE: Cache = Default::default();
        }

        let mut hit = true;
        let output = CACHE.with(|cache| {
            cache
                .borrow()
                .iter()
                .find(|(tracker, _)| {
                    let (#arg, _) = ::comemo::internal::to_parts(#arg);
                    #trackable::valid(#arg, tracker)
                })
                .map(|&(_, output)| output)
        });

        let output = output.unwrap_or_else(|| {
            let tracker = ::core::default::Default::default();
            let (#arg, _) = ::comemo::internal::to_parts(#arg);
            let #arg = ::comemo::internal::from_parts(#arg, Some(&tracker));
            let output = inner(#arg);
            CACHE.with(|cache| cache.borrow_mut().push((tracker, output)));
            hit = false;
            output
        });

        println!(
            "{} {} {}",
            #name,
            if hit { "[hit]: " } else { "[miss]:" },
            output,
        );

        output
    };

    let mut outer = func.clone();
    outer.block = parse_quote! { {
        #inner
        { #body }
    } };

    Ok(quote! { #outer })
}
