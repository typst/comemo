extern crate proc_macro;

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

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_quote, Error, Result};

/// Memoize a pure function.
///
/// ```
/// #[memoize]
/// fn describe(image: Tracked<Image>) -> String {
///     let w = image.width();
///     let h = image.height();
///     format!("The image is {w}x{h} pixels.")
/// }
/// ```
#[proc_macro_attribute]
pub fn memoize(_: TokenStream, stream: TokenStream) -> TokenStream {
    let func = syn::parse_macro_input!(stream as syn::ItemFn);
    memoize::expand(func)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Make a type trackable.
///
/// ```
/// /// A decoded raster image.
/// pub struct Image {
///     width: u32,
///     height: u32,
///     pixels: Vec<u8>,
/// }
///
/// #[track]
/// impl Image {
///     fn width(&self) -> u32 {
///         self.width
///     }
///
///     fn height(&self) -> u32 {
///         self.height
///     }
/// }
/// ```
///
/// [`Track`]: ../comemo/trait.Track.html
#[proc_macro_attribute]
pub fn track(_: TokenStream, stream: TokenStream) -> TokenStream {
    let block = syn::parse_macro_input!(stream as syn::ItemImpl);
    track::expand(block)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
