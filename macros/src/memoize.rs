use utils::parse_key_value;

use super::*;

/// Memoize a function.
pub fn expand(attrs: TokenStream, item: &syn::Item) -> Result<proc_macro2::TokenStream> {
    let syn::Item::Fn(item) = item else {
        bail!(item, "`memoize` can only be applied to functions and methods");
    };

    // Preprocess and validate the function.
    let function = prepare(attrs, item)?;

    // Rewrite the function's body to memoize it.
    process(&function)
}

/// Details about a function that should be memoized.
struct Function {
    item: syn::ItemFn,
    args: Vec<Argument>,
    output: syn::Type,
    enabled: Option<syn::Expr>,
}

/// Additional metadata for a memoized function.
struct Meta {
    enabled: Option<syn::Expr>,
}

impl syn::parse::Parse for Meta {
    fn parse(input: syn::parse::ParseStream) -> Result<Self> {
        Ok(Self { enabled: parse_key_value::<kw::enabled, _>(input)? })
    }
}

/// An argument to a memoized function.
enum Argument {
    Receiver(syn::Token![self]),
    Ident(Box<syn::Type>, Option<syn::Token![mut]>, syn::Ident),
}

/// Preprocess and validate a function.
fn prepare(attrs: TokenStream, function: &syn::ItemFn) -> Result<Function> {
    let meta = syn::parse2::<Meta>(attrs.clone())?;

    let mut args = vec![];
    for input in &function.sig.inputs {
        args.push(prepare_arg(input)?);
    }

    let output = match &function.sig.output {
        syn::ReturnType::Default => parse_quote! { () },
        syn::ReturnType::Type(_, ty) => ty.as_ref().clone(),
    };

    Ok(Function {
        item: function.clone(),
        args,
        output,
        enabled: meta.enabled,
    })
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

            Argument::Ident(typed.ty.clone(), *mutability, ident.clone())
        }
    })
}

/// Rewrite a function's body to memoize it.
fn process(function: &Function) -> Result<TokenStream> {
    // Construct assertions that the arguments fulfill the necessary bounds.
    let bounds = function.args.iter().map(|arg| {
        let val = match arg {
            Argument::Receiver(token) => quote! { #token },
            Argument::Ident(_, _, ident) => quote! { #ident },
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
        Argument::Ident(_, _, ident) => quote! { #ident },
    });
    let arg_tuple = quote! { (#(#args,)*) };

    let arg_tys = function.args.iter().map(|arg| match arg {
        Argument::Receiver(_) => quote! { () },
        Argument::Ident(ty, _, _) => quote! { #ty },
    });
    let arg_ty_tuple = quote! { (#(#arg_tys,)*) };

    // Construct a tuple for all parameters.
    let params = function.args.iter().map(|arg| match arg {
        Argument::Receiver(_) => quote! { _ },
        Argument::Ident(_, mutability, ident) => quote! { #mutability #ident },
    });
    let param_tuple = quote! { (#(#params,)*) };

    // Construct the inner closure.
    let output = &function.output;
    let body = &function.item.block;
    let closure = quote! { |::comemo::internal::Multi(#param_tuple)| -> #output #body };

    // Adjust the function's body.
    let mut wrapped = function.item.clone();
    for arg in wrapped.sig.inputs.iter_mut() {
        let syn::FnArg::Typed(typed) = arg else { continue };
        let syn::Pat::Ident(ident) = typed.pat.as_mut() else { continue };
        ident.mutability = None;
    }

    let enabled = function.enabled.clone().unwrap_or(parse_quote! { true });

    wrapped.block = parse_quote! { {
        static __CACHE: ::comemo::internal::Cache<
            <::comemo::internal::Multi<#arg_ty_tuple> as ::comemo::internal::Input>::Call,
            #output,
        > = ::comemo::internal::Cache::new(|| {
            ::comemo::internal::register_evictor(|max_age| __CACHE.evict(max_age));
            ::core::default::Default::default()
        });

        #(#bounds;)*

        ::comemo::internal::memoize(
            &__CACHE,
            ::comemo::internal::Multi(#arg_tuple),
            &mut ::core::default::Default::default(),
            #enabled,
            #closure,
        )
    } };

    Ok(quote! { #wrapped })
}

pub mod kw {
    syn::custom_keyword!(enabled);
}
