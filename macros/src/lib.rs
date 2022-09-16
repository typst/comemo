extern crate proc_macro;

/// Return an error at the given item.
macro_rules! bail {
    ($item:expr, $fmt:literal $($tts:tt)*) => {
        return Err(Error::new_spanned(
            &$item,
            format!(concat!("comemo: ", $fmt) $($tts)*)
        ))
    }
}

mod memoize;
mod track;

use proc_macro::TokenStream as BoundaryStream;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::{parse_quote, Error, Result};

/// Memoize a pure function.
#[proc_macro_attribute]
pub fn memoize(_: BoundaryStream, stream: BoundaryStream) -> BoundaryStream {
    let func = syn::parse_macro_input!(stream as syn::Item);
    memoize::expand(&func)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Make a type trackable.
#[proc_macro_attribute]
pub fn track(_: BoundaryStream, stream: BoundaryStream) -> BoundaryStream {
    let block = syn::parse_macro_input!(stream as syn::Item);
    track::expand(&block)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
