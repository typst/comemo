//! Tracked memoization.

// These are implementation details. Do not rely on them!
#[doc(hidden)]
pub mod internal;

pub use comemo_macros::{memoize, track};

use std::ops::Deref;

/// A trackable type.
///
/// This is implemented by types that have an implementation block annoted with
/// [`#[track]`](track).
pub trait Track<'a>: internal::Trackable<'a> {
    /// Start tracking a value.
    fn track(&'a self) -> Tracked<'a, Self> {
        Tracked { inner: self, tracker: None }
    }
}

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
pub struct Tracked<'a, T>
where
    T: Track<'a>,
{
    /// A reference to the tracked value.
    inner: &'a T,
    /// A tracker which stores constraints for T. It is filled by the tracked
    /// methods on T's generated surface type.
    ///
    /// Starts out as `None` and is set to a stack-stored tracker in the
    /// preamble of memoized functions.
    tracker: Option<&'a T::Tracker>,
}

// The type `Tracked<T>` automatically dereferences to T's generated surface
// type. This makes all tracked methods available, but leaves all other ones
// unaccessible.
impl<'a, T> Deref for Tracked<'a, T>
where
    T: Track<'a>,
{
    type Target = T::Surface;

    fn deref(&self) -> &Self::Target {
        T::surface(self)
    }
}

impl<'a, T> Copy for Tracked<'a, T> where T: Track<'a> {}

impl<'a, T> Clone for Tracked<'a, T>
where
    T: Track<'a>,
{
    fn clone(&self) -> Self {
        *self
    }
}
