//! Tracked memoization.

mod cache;
mod hash;
mod input;
mod track;

pub use crate::track::{Track, Tracked};
pub use comemo_macros::{memoize, track};

/// These are implementation details. Do not rely on them!
#[doc(hidden)]
pub mod internal {
    pub use crate::cache::CACHE;
    pub use crate::hash::HashConstraint;
    pub use crate::input::{assert_hashable_or_trackable, Args};
    pub use crate::track::{from_parts, to_parts, Trackable};

    /// Helper trait for lifetime type families.
    pub trait Family<'a> {
        /// The concrete type with lifetime 'a.
        type Out;
    }
}
