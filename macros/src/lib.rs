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

/// Memoize a function.
///
/// Memoized functions can take two kinds of arguments:
/// - _Hashed:_ This is the default. These arguments are hashed into a
///   high-quality 128-bit hash, which is used as a cache key. For this to be
///   correct, the hash implementations of your arguments **must feed all the
///   information your arguments expose to the hasher**. Otherwise, memoized
///   results might get reused invalidly.
/// - _Tracked:_ The argument is of the form `Tracked<T>`. These arguments enjoy
///   fine-grained access tracking and needn't be exactly the same for a cache
///   hit to occur. They only need to be used equivalently.
///
/// You can also add the `#[memoize]` attribute to methods in inherent and trait
/// impls.
///
/// # Example
/// ```
/// /// Evaluate a `.calc` script.
/// #[comemo::memoize]
/// fn evaluate(script: &str, files: comemo::Tracked<Files>) -> i32 {
///     script
///         .split('+')
///         .map(str::trim)
///         .map(|part| match part.strip_prefix("eval ") {
///             Some(path) => evaluate(&files.read(path), files),
///             None => part.parse::<i32>().unwrap(),
///         })
///         .sum()
/// }
/// ```
///
/// # Restrictions
/// There are certain restrictions that apply to memoized functions. Most of
/// these are checked by comemo, but some are your responsibility:
/// - They must be **pure**, that is, **free of observable side effects**. This
///   is **your reponsibility** as comemo can't check it.
/// - They must have an explicit return type.
/// - They cannot have mutable parameters (conflicts with purity).
/// - They cannot use destructuring patterns in their arguments.
#[proc_macro_attribute]
pub fn memoize(_: BoundaryStream, stream: BoundaryStream) -> BoundaryStream {
    let func = syn::parse_macro_input!(stream as syn::Item);
    memoize::expand(&func)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Make a type trackable.
///
/// Adding this to an impl block of a type `T`, implements the `Track` trait for
/// `T`. This lets you call `.track()` on that type, producing a `Tracked<T>`.
/// When such a tracked type is used an argument to a memoized function it
/// enjoys fine-grained access tracking instead of being bluntly hashed.
///
/// You can also add the `#[track]` attribute to a trait to make its trait
/// object trackable.
///
/// # Example
/// ```
/// /// File storage.
/// struct Files(HashMap<PathBuf, String>);
///
/// #[comemo::track]
/// impl Files {
///     /// Load a file from storage.
///     fn read(&self, path: &str) -> String {
///         self.0.get(Path::new(path)).cloned().unwrap_or_default()
///     }
/// }
///
/// impl Files {
///     /// Write a file to storage.
///     fn write(&mut self, path: &str, text: &str) {
///         self.0.insert(path.into(), text.into());
///     }
/// }
/// ```
///
/// # Restrictions
/// Tracked impl blocks or traits may not be generic and may only contain
/// methods. Just like with memoized functions, certain restrictions apply to
/// tracked methods:
/// - They must be **pure**, that is, **free of observable side effects**. This
///   is **your reponsibility** as comemo can't check it. You can use interior
///   mutability as long as the method stays idempotent.
/// - They cannot be generic.
/// - They can only be private or public not `pub(...)`.
/// - They cannot be `unsafe`, `async` or `const`.
/// - They must take an `&self` parameter.
/// - They must have an explicit return type.
/// - They cannot have mutable parameters (conflicts with purity).
/// - They cannot use destructuring patterns in their arguments.
#[proc_macro_attribute]
pub fn track(_: BoundaryStream, stream: BoundaryStream) -> BoundaryStream {
    let block = syn::parse_macro_input!(stream as syn::Item);
    track::expand(&block)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
