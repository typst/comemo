/*!
Memoization on steroids.

A _memoized_ function caches its return values so that it only needs to be
executed once per set of unique arguments.
While this only works for _pure_ functions (functions that don't have side
effects), it can improve performance in many computations.

However, memoization is not a silver bullet.
In many cases, the cache hit rate isn't all that great because arguments are
ever so slightly different.
This is where comemo comes into play:
It let's you reuse a function's output even when called with
_different_ arguments as long as the difference is not observed by the function.
It achieves this by carefully tracking which parts of an argument were actually
accessed during the invocation of a memoized function.
*/

mod cache;
mod constraint;
mod input;
mod prehashed;
mod track;

pub use crate::cache::{config, evict, Config};
pub use crate::prehashed::Prehashed;
pub use crate::track::{Track, Tracked};
pub use comemo_macros::{memoize, track};

/// These are implementation details. Do not rely on them!
#[doc(hidden)]
pub mod internal {
    pub use crate::cache::memoized;
    pub use crate::constraint::{hash, Join, MultiConstraint, SoloConstraint};
    pub use crate::input::{assert_hashable_or_trackable, Args};
    pub use crate::track::{to_parts, Trackable};

    /// Helper trait for lifetime type families.
    pub trait Family<'a> {
        /// The concrete type with lifetime 'a.
        type Out;
    }
}
