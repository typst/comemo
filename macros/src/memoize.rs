use super::*;

/// Memoize a function.
pub fn expand(item: &syn::Item) -> Result<proc_macro2::TokenStream> {
    let item = match item {
        syn::Item::Fn(item) => item,
        _ => bail!(
            item,
            "`memoize` can only be applied to functions and methods"
        ),
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
    Ident(syn::Ident),
    Receiver(syn::Token![self]),
}

impl ToTokens for Argument {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::Ident(ident) => ident.to_tokens(tokens),
            Self::Receiver(token) => token.to_tokens(tokens),
        }
    }
}

/// Preprocess and validate a function.
fn prepare(function: &syn::ItemFn) -> Result<Function> {
    let mut args = vec![];

    for input in &function.sig.inputs {
        args.push(prepare_arg(input)?);
    }

    let output = match &function.sig.output {
        syn::ReturnType::Default => {
            bail!(function.sig, "memoized function must have a return type")
        }
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
            let name = match typed.pat.as_ref() {
                syn::Pat::Ident(syn::PatIdent {
                    by_ref: None,
                    mutability: None,
                    ident,
                    subpat: None,
                    ..
                }) => ident.clone(),
                pat => bail!(pat, "only simple identifiers are supported"),
            };

            let ty = typed.ty.as_ref().clone();
            match ty {
                syn::Type::Reference(syn::TypeReference {
                    mutability: Some(_), ..
                }) => {
                    bail!(
                        typed.ty,
                        "memoized functions cannot have mutable parameters"
                    )
                }
                _ => {}
            }

            Argument::Ident(name)
        }
    })
}

/// Rewrite a function's body to memoize it.
fn process(function: &Function) -> Result<TokenStream> {
    // Construct assertions that the arguments fulfill the necessary bounds.
    let bounds = function.args.iter().map(|arg| {
        quote_spanned! { function.item.span() =>
            ::comemo::internal::assert_hashable_or_trackable(&#arg);
        }
    });

    // Construct a tuple from all arguments.
    let args = function.args.iter().map(|arg| match arg {
        Argument::Ident(id) => id.to_token_stream(),
        Argument::Receiver(token) => quote! {
            ::comemo::internal::hash(&#token)
        },
    });
    let arg_tuple = quote! { (#(#args,)*) };

    // Construct a tuple for all parameters.
    let params = function.args.iter().map(|arg| match arg {
        Argument::Ident(id) => id.to_token_stream(),
        Argument::Receiver(_) => quote! { _ },
    });
    let param_tuple = quote! { (#(#params,)*) };

    // Construct the inner closure.
    let output = &function.output;
    let body = &function.item.block;
    let closure = quote! { |#param_tuple| -> #output #body };

    // Adjust the function's body.
    let mut wrapped = function.item.clone();
    let unique = quote! { __ComemoUnique };

    wrapped.block = parse_quote! { {
        struct #unique;
        #(#bounds;)*
        ::comemo::internal::memoized(
            ::core::any::TypeId::of::<#unique>(),
            ::comemo::internal::Args(#arg_tuple),
            #closure,
        )
    } };

    Ok(quote! { #wrapped })
}
