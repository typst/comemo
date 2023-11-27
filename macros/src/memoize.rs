use super::*;

/// Memoize a function.
pub fn expand(item: &syn::Item) -> Result<proc_macro2::TokenStream> {
    let syn::Item::Fn(item) = item else {
        bail!(item, "`memoize` can only be applied to functions and methods");
    };

    // Preprocess and validate the function.
    let function = prepare(&item)?;

    // Rewrite the function's body to memoize it.
    process(&function)
}

/// Details about a function that should be memoized.
struct Function {
    item: syn::ItemFn,
    args: Vec<Argument>,
    output: syn::Type,
}

/// An argument to a memoized function.
enum Argument {
    Receiver(syn::Token![self]),
    Ident(Option<syn::Token![mut]>, syn::Ident),
}

/// Preprocess and validate a function.
fn prepare(function: &syn::ItemFn) -> Result<Function> {
    let mut args = vec![];

    for input in &function.sig.inputs {
        args.push(prepare_arg(input)?);
    }

    let output = match &function.sig.output {
        syn::ReturnType::Default => parse_quote! { () },
        syn::ReturnType::Type(_, ty) => ty.as_ref().clone(),
    };

    Ok(Function { item: function.clone(), args, output })
}

/// Preprocess a function argument.
fn prepare_arg(input: &syn::FnArg) -> Result<Argument> {
    Ok(match input {
        syn::FnArg::Receiver(recv) => {
            if recv.mutability.is_some() {
                bail!(recv, "memoized functions cannot have mutable parameters");
            }

            Argument::Receiver(recv.self_token)
        }
        syn::FnArg::Typed(typed) => {
            let syn::Pat::Ident(syn::PatIdent {
                by_ref: None,
                mutability,
                ident,
                subpat: None,
                ..
            }) = typed.pat.as_ref()
            else {
                bail!(typed.pat, "only simple identifiers are supported");
            };

            if let syn::Type::Reference(syn::TypeReference {
                mutability: Some(_), ..
            }) = typed.ty.as_ref()
            {
                bail!(typed.ty, "memoized functions cannot have mutable parameters")
            }

            Argument::Ident(mutability.clone(), ident.clone())
        }
    })
}

/// Rewrite a function's body to memoize it.
fn process(function: &Function) -> Result<TokenStream> {
    // Construct assertions that the arguments fulfill the necessary bounds.
    let bounds = function.args.iter().map(|arg| {
        let val = match arg {
            Argument::Receiver(token) => quote! { #token },
            Argument::Ident(_, ident) => quote! { #ident },
        };
        quote_spanned! { function.item.span() =>
            ::comemo::internal::assert_hashable_or_trackable(&#val);
        }
    });

    // Construct a tuple from all arguments.
    let args = function.args.iter().map(|arg| match arg {
        Argument::Receiver(token) => quote! {
            ::comemo::internal::hash(&#token)
        },
        Argument::Ident(_, ident) => quote! { #ident },
    });
    let arg_tuple = quote! { (#(#args,)*) };

    // Construct a tuple for all parameters.
    let params = function.args.iter().map(|arg| match arg {
        Argument::Receiver(_) => quote! { _ },
        Argument::Ident(mutability, ident) => quote! { #mutability #ident },
    });
    let param_tuple = quote! { (#(#params,)*) };

    // Construct the inner closure.
    let output = &function.output;
    let body = &function.item.block;
    let closure = quote! { |#param_tuple| -> #output #body };

    // Adjust the function's body.
    let mut wrapped = function.item.clone();
    for arg in wrapped.sig.inputs.iter_mut() {
        let syn::FnArg::Typed(typed) = arg else { continue };
        let syn::Pat::Ident(ident) = typed.pat.as_mut() else { continue };
        ident.mutability = None;
    }

    let unique = quote! { __ComemoUnique };
    wrapped.block = parse_quote! { {
        struct #unique;
        #(#bounds;)*
        ::comemo::internal::memoized(
            ::core::any::TypeId::of::<#unique>(),
            ::comemo::internal::Args(#arg_tuple),
            &::core::default::Default::default(),
            #closure,
        )
    } };

    Ok(quote! { #wrapped })
}
