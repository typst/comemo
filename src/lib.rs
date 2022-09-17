//! Tracked memoization.

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
