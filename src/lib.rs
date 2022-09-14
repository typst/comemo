//! Tracked memoization.

mod cache;
mod hash;
mod track;

pub use crate::track::{Track, Tracked};
pub use comemo_macros::{memoize, track};

/// These are implementation details. Do not rely on them!
#[doc(hidden)]
pub mod internal {
    pub use crate::cache::CACHE;
    pub use crate::hash::HashConstraint;
    pub use crate::track::{from_parts, to_parts, Family, Trackable};
}
