//! Tracked memoization.

pub use comemo_macros::{memoize, track};

use std::hash::Hash;
use std::ops::Deref;

use siphasher::sip128::{Hasher128, SipHasher};

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
pub struct Tracked<'a, T>(<T as Track<'a>>::Surface)
where
    T: Track<'a>;

/// A trackable type.
pub trait Track<'a>: Sized + 'a {
    /// Start tracking a value.
    fn track(&'a self) -> Tracked<Self> {
        Tracked(Self::Surface::from(self))
    }

    /// The tracked API surface of the type.
    type Surface: From<&'a Self>;

    /// The tracking constraint.
    type Constraint: Default;
}

impl<'a, T> Deref for Tracked<'a, T>
where
    T: Track<'a>,
{
    type Target = T::Surface;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

/// Validate a type against a constraint.
pub trait Validate {
    /// The constraint generated for validation of this type.
    type Constraint;

    /// Generate a constraint for this type.
    ///
    /// The new constraint might hold all relevant information from the
    /// beginning (hash constraint) or be updated over course of a memoized
    /// function's execution based on how the type is used (tracked constraint).
    fn constraint(&self) -> Self::Constraint;

    /// Validate an instance of this type against a constraint.
    fn valid(&self, ct: &Self::Constraint) -> bool;
}

impl<T> Validate for T
where
    T: Hash,
{
    type Constraint = u128;

    fn constraint(&self) -> Self::Constraint {
        let mut state = SipHasher::new();
        self.hash(&mut state);
        state.finish128().as_u128()
    }

    fn valid(&self, ct: &Self::Constraint) -> bool {
        self.constraint() == *ct
    }
}

impl<'a, T> Validate for Tracked<'a, T>
where
    T: Track<'a>,
{
    type Constraint = <T as Track<'a>>::Constraint;

    fn constraint(&self) -> Self::Constraint {
        <T as Track>::Constraint::default()
    }

    fn valid(&self, _: &Self::Constraint) -> bool {
        false
    }
}
