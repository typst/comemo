use super::*;

/// Memoize a function.
pub fn expand(item: &syn::ItemFn) -> Result<proc_macro2::TokenStream> {
    // Preprocess and validate the function.
    let function = prepare(&item)?;

    // Rewrite the function's body to memoize it.
    process(&function)
}

/// Details about a function that should be memoized.
struct Function {
    item: syn::ItemFn,
    name: syn::Ident,
    args: Vec<syn::Ident>,
    types: Vec<syn::Type>,
    output: syn::Type,
}

/// Preprocess and validate a function.
fn prepare(function: &syn::ItemFn) -> Result<Function> {
    let mut args = vec![];
    let mut types = vec![];

    for input in &function.sig.inputs {
        let typed = match input {
            syn::FnArg::Typed(typed) => typed,
            syn::FnArg::Receiver(_) => {
                bail!(function, "methods are not supported")
            }
        };

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
        args.push(name);
        types.push(ty);
    }

    let output = match &function.sig.output {
        syn::ReturnType::Default => {
            bail!(function.sig, "function must have a return type")
        }
        syn::ReturnType::Type(_, ty) => ty.as_ref().clone(),
    };

    Ok(Function {
        item: function.clone(),
        name: function.sig.ident.clone(),
        args,
        types,
        output,
    })
}

/// Rewrite a function's body to memoize it.
fn process(function: &Function) -> Result<TokenStream> {
    // Construct a tuple from all arguments.
    let args = &function.args;
    let arg_tuple = quote! { (#(#args,)*) };

    // Construct assertions that the arguments fulfill the necessary bounds.
    let bounds = function.types.iter().map(|ty| {
        quote! {
            ::comemo::internal::assert_hashable_or_trackable::<#ty>();
        }
    });

    // Construct the inner closure.
    let output = &function.output;
    let body = &function.item.block;
    let closure = quote! { |#arg_tuple| -> #output #body };

    // Adjust the function's body.
    let mut wrapped = function.item.clone();
    let name = function.name.to_string();
    wrapped.block = parse_quote! { {
        #(#bounds;)*
        ::comemo::internal::cached(
            #name,
            ::comemo::internal::Args(#arg_tuple),
            #closure,
        )
    } };

    Ok(quote! { #wrapped })
}
