/*!
Tracked memoization.
*/

pub use comemo_macros::{memoize, track};

// This is an implementation detail, which shouldn't directly be used.
#[doc(hidden)]
pub use {once_cell, siphasher};

use std::ops::Deref;

/// Tracks accesses to a value.
///
/// Encapsulates a reference to a value and tracks all accesses to it.
/// The only methods accessible on `Tracked<T>` are those defined in an impl
/// block for `T` annotated with [`#[track]`](track).
///
/// ```
/// use comemo::Track;
///
/// let image = Image::random(20, 40);
/// let sentence = describe(image.track());
/// println!("{sentence}");
/// ```
pub struct Tracked<'a, T>(&'a T)
where
    T: Track;

/// A trackable type.
pub trait Track: Sized {
    /// Start tracking a value.
    fn track(&self) -> Tracked<Self> {
        Tracked(self)
    }

    // Everything below is an implementation detail.
    // You shouldn't use that stuff directly.

    /// The tracked API surface of the type.
    #[doc(hidden)]
    type Surface;

    /// Access the tracked API surface.
    #[doc(hidden)]
    fn surface(&self) -> &Self::Surface;
}

impl<'a, T> Deref for Tracked<'a, T>
where
    T: Track,
{
    type Target = T::Surface;

    fn deref(&self) -> &Self::Target {
        self.0.surface()
    }
}
