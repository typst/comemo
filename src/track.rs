use std::fmt::{self, Debug, Formatter};
use std::ops::Deref;

use crate::constraint::Join;
use crate::internal::Family;

/// Tracks accesses to a value.
///
/// Encapsulates a reference to a value and tracks all accesses to it. The only
/// methods accessible on `Tracked<T>` are those defined in an implementation
/// block for `T` annotated with [`#[track]`](macro@crate::track).
pub struct Tracked<'a, T>
where
    T: Track + ?Sized,
{
    /// A reference to the tracked value.
    value: &'a T,
    /// A constraint that is generated for T by the tracked methods on T's
    /// surface type.
    ///
    /// Starts out as `None` and is set to a stack-stored constraint in the
    /// preamble of memoized functions.
    constraint: Option<&'a T::Constraint>,
}

// The type `Tracked<T>` automatically dereferences to T's generated surface
// type. This makes all tracked methods available, but leaves all other ones
// unaccessible.
impl<'a, T> Deref for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    type Target = <T::Surface as Family<'a>>::Out;

    #[inline]
    fn deref(&self) -> &Self::Target {
        T::surface(self)
    }
}

impl<'a, T> Copy for Tracked<'a, T> where T: Track + ?Sized {}

impl<'a, T> Clone for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    #[inline]
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Debug for Tracked<'_, T>
where
    T: Track + ?Sized,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.pad("Tracked(..)")
    }
}

/// A trackable type.
///
/// This is implemented by types that have an implementation block annoted with
/// [`#[track]`](macro@crate::track).
pub trait Track: Trackable {
    /// Start tracking a value.
    #[inline]
    fn track(&self) -> Tracked<Self> {
        Tracked { value: self, constraint: None }
    }
}

/// Non-exposed parts of the `Track` trait.
pub trait Trackable: 'static {
    /// Describes an instance of type.
    type Constraint: Default + Debug + Join + 'static;

    /// The tracked API surface of this type.
    type Surface: for<'a> Family<'a>;

    /// Whether an instance fulfills the given constraint.
    fn valid(&self, constraint: &Self::Constraint) -> bool;

    /// Cast a reference from `Tracked` to this type's surface.
    fn surface<'a, 'r>(
        tracked: &'r Tracked<'a, Self>,
    ) -> &'r <Self::Surface as Family<'a>>::Out
    where
        Self: Track;
}

/// Destructure a `Tracked<_>` into its parts.
#[inline]
pub fn to_parts<T>(tracked: Tracked<T>) -> (&T, Option<&T::Constraint>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}

/// Create a `Tracked<_>` from its parts.
#[inline]
pub fn from_parts<'a, T>(
    value: &'a T,
    constraint: Option<&'a T::Constraint>,
) -> Tracked<'a, T>
where
    T: Track + ?Sized,
{
    Tracked { value, constraint }
}
