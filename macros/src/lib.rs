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
mod utils;

use proc_macro::TokenStream as BoundaryStream;
use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Error, Result, parse_quote};

/// Memoize a function.
///
/// This attribute can be applied to free-standing functions as well as methods
/// in inherent and trait impls.
///
/// # Kinds of arguments
/// Memoized functions can take three different kinds of arguments:
///
/// - _Hashed:_ This is the default. These arguments are hashed into a
///   high-quality 128-bit hash, which is used as a cache key.
///
/// - _Immutably tracked:_ The argument is of the form `Tracked<T>`. These
///   arguments enjoy fine-grained access tracking. This allows cache hits to
///   occur even if the value of `T` is different than previously as long as the
///   difference isn't observed.
///
/// - _Mutably tracked:_  The argument is of the form `TrackedMut<T>`. Through
///   this type, you can safely mutate an argument from within a memoized
///   function. If there is a cache hit, comemo will replay all mutations.
///   Mutable tracked methods cannot have return values.
///
/// # Restrictions
/// The following restrictions apply to memoized functions:
///
/// - For the memoization to be correct, the [`Hash`](std::hash::Hash)
///   implementations of your arguments **must feed all the information they
///   expose to the hasher**. Otherwise, memoized results might get reused
///   invalidly.
///
/// - The **only observable impurity memoized functions may exhibit are
///   mutations through `TrackedMut<T>` arguments.** Comemo stops you from using
///   basic mutable arguments, but it cannot determine all sources of impurity,
///   so this is your responsibility.
///
/// - Memoized functions must **call tracked methods in _reorderably
///   deterministic_ fashion.** Consider two executions A and B of a memoized
///   function. We define the following two properties:
///
///   - _In-order deterministic:_ If the first N tracked calls and their results
///     are the same in A and B, then the N+1th call must also be the same. This
///     is a fairly natural property as far as deterministic functions go, as,
///     if the first N calls and their results were the same across two
///     execution, the available information for choosing the N+1th call is the
///     same. However, this property is a bit too restrictive in practice. For
///     instance, a function that internally uses multi-threading may call
///     tracked methods out-of-order while still producing a deterministic
///     result.
///
///   - _Reorderably deterministic:_ If, for the first N calls in A, B has
///     matching calls (same arguments, same return value) somewhere in its call
///     sequence, then the N+1th call invoked by A must also occur _somewhere_
///     in the call sequence of B. This is a somewhat relaxed version of
///     in-order determinism that still allows comemo to perform internal
///     optimizations while permitting memoization of many more functions (e.g.
///     ones that use internal multi-threading in an outwardly deterministic
///     fashion).
///
///   Reorderable determinism is necessary for efficient cache lookups. If a
///   memoized function is not reorderably determinstic, comemo may panic in
///   debug mode to bring your attention to this. Meanwhile, in release mode,
///   memoized functions will still yield correct results, but caching may prove
///   ineffective.
///
/// - The output of a memoized function must be `Send` and `Sync` because it is
///   stored in the global cache.
///
/// Furthermore, memoized functions cannot use destructuring patterns in their
/// arguments.
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
/// # Disabling memoization conditionally
/// If you want to enable or disable memoization for a function conditionally,
/// you can use the `enabled` attribute. This is useful for cheap function calls
/// where dealing with the caching is more expensive than recomputing the
/// result. This allows you to bypass hashing and constraint validation while
/// still dealing with the same function signature. And allows saving memory and
/// time.
///
/// By default, all functions are unconditionally memoized. To disable
/// memoization conditionally, you must specify an `enabled = <expr>` attribute.
/// The expression can use the parameters and must evaluate to a boolean value.
/// If the expression is `false`, the function will be executed without hashing
/// and caching.
///
/// ## Example
/// ```
/// /// Compute the sum of a slice of floats, but only memoize if the slice is
/// /// longer than 1024 elements.
/// #[comemo::memoize(enabled = add.len() > 1024)]
/// fn evaluate(add: &[f32]) -> f32 {
///     add.iter().copied().sum()
/// }
/// ```
///
#[proc_macro_attribute]
pub fn memoize(args: BoundaryStream, stream: BoundaryStream) -> BoundaryStream {
    let args = syn::parse_macro_input!(args as TokenStream);
    let func = syn::parse_macro_input!(stream as syn::Item);
    memoize::expand(args, &func)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}

/// Make a type trackable.
///
/// This attribute can be applied to an inherent implementation block or trait
/// definition. It implements the `Track` trait for the type or trait object.
///
/// # Tracking immutably and mutably
/// This allows you to
///
/// - call `track()` on that type, producing a `Tracked<T>` container. Used as
///   an argument to a memoized function, these containers enjoy fine-grained
///   access tracking instead of blunt hashing.
///
/// - call `track_mut()` on that type, producing a `TrackedMut<T>`. For mutable
///   arguments, tracking is the only option, so that comemo can replay the side
///   effects when there is a cache hit.
///
/// # Restrictions
/// Tracked impl blocks or traits may not be generic and may only contain
/// methods. Just like with memoized functions, certain restrictions apply to
/// tracked methods:
///
/// - The **only obversable impurity tracked methods may exhibit are mutations
///   through `&mut self`.** Comemo stops you from using basic mutable arguments
///   and return values, but it cannot determine all sources of impurity, so
///   this is your responsibility. Tracked methods also must not return mutable
///   references or other types which allow untracked mutation. You _are_
///   allowed to use interior mutability if it is not observable (even in
///   immutable methods, as long as they stay idempotent).
///
/// - The return values of tracked methods must implement
///   [`Hash`](std::hash::Hash) and **must feed all the information they expose
///   to the hasher**. Otherwise, memoized results might get reused invalidly.
///
/// - Mutable tracked methods must not have a return value.
///
/// - A tracked implementation cannot have a mix of mutable and immutable
///   methods.
///
/// - The arguments to a tracked method must be `Send` and `Sync` because they
///   are stored in the global cache.
///
/// Furthermore:
/// - Tracked methods cannot be generic.
/// - They cannot be `unsafe`, `async` or `const`.
/// - They must take an `&self` or `&mut self` parameter.
/// - Their arguments must implement [`ToOwned`].
/// - Their return values must implement [`Hash`](std::hash::Hash).
/// - They cannot use destructuring patterns in their arguments.
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
#[proc_macro_attribute]
pub fn track(_: BoundaryStream, stream: BoundaryStream) -> BoundaryStream {
    let block = syn::parse_macro_input!(stream as syn::Item);
    track::expand(&block)
        .unwrap_or_else(|err| err.to_compile_error())
        .into()
}
