use std::fmt::{self, Debug, Formatter};
use std::ops::{Deref, DerefMut};

use crate::constraint::Constraint;
use crate::internal::Family;

/// A trackable type.
///
/// This is implemented by types that have an implementation block annotated
/// with `#[track]` and for trait objects whose traits are annotated with
/// `#[track]`. For more details, see [its documentation](macro@crate::track).
pub trait Track: Trackable {
    /// Start tracking all accesses to a value.
    #[inline]
    fn track(&self) -> Tracked<Self> {
        Tracked { value: self, constraint: None }
    }

    /// Start tracking all accesses and mutations to a value.
    #[inline]
    fn track_mut(&mut self) -> TrackedMut<Self> {
        TrackedMut { value: self, constraint: None }
    }
}

/// Non-exposed parts of the `Track` trait.
pub trait Trackable: 'static {
    /// Encodes a function call on this type.
    type Call: Clone + PartialEq + 'static;

    /// The tracked API surface of this type.
    type Surface: for<'a> Family<'a>;

    /// The mutable tracked API surface of this type.
    type SurfaceMut: for<'a> Family<'a>;

    /// Whether an instance fulfills the given constraint.
    fn valid(&self, constraint: &Constraint<Self::Call>) -> bool;

    /// Replay mutations to the value.
    fn replay(&mut self, constraint: &Constraint<Self::Call>);

    /// Access the immutable surface from a `Tracked`.
    fn surface_ref<'a, 't>(
        tracked: &'t Tracked<'a, Self>,
    ) -> &'t <Self::Surface as Family<'a>>::Out
    where
        Self: Track;

    /// Access the immutable surface from a `TrackedMut`.
    fn surface_mut_ref<'a, 't>(
        tracked: &'t TrackedMut<'a, Self>,
    ) -> &'t <Self::SurfaceMut as Family<'a>>::Out
    where
        Self: Track;

    /// Access the mutable surface from a `TrackedMut`.
    fn surface_mut_mut<'a, 't>(
        tracked: &'t mut TrackedMut<'a, Self>,
    ) -> &'t mut <Self::SurfaceMut as Family<'a>>::Out
    where
        Self: Track;
}

/// Tracks accesses to a value.
///
/// Encapsulates a reference to a value and tracks all accesses to it. The only
/// methods accessible on `Tracked<T>` are those defined in an implementation
/// block or trait for `T` annotated with `#[track]`. For more details, see [its
/// documentation](macro@crate::track).
pub struct Tracked<'a, T>
where
    T: Track + ?Sized,
{
    /// A reference to the tracked value.
    pub(crate) value: &'a T,
    /// A constraint that is generated for T by the tracked methods on T's
    /// surface type.
    ///
    /// Starts out as `None` and is set to a stack-stored constraint in the
    /// preamble of memoized functions.
    pub(crate) constraint: Option<&'a Constraint<T::Call>>,
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
        T::surface_ref(self)
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

/// Tracks accesses and mutations to a value.
///
/// Encapsulates a mutable reference to a value and tracks all accesses to it.
/// The only methods accessible on `TrackedMut<T>` are those defined in an
/// implementation block or trait for `T` annotated with `#[track]`. For more
/// details, see [its documentation](macro@crate::track).
pub struct TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    /// A reference to the tracked value.
    pub(crate) value: &'a mut T,
    /// A constraint that is generated for T by the tracked methods on T's
    /// surface type.
    ///
    /// Starts out as `None` and is set to a stack-stored constraint in the
    /// preamble of memoized functions.
    pub(crate) constraint: Option<&'a Constraint<T::Call>>,
}

impl<'a, T> TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    /// Downgrade to an immutable reference.
    ///
    /// This is an associated function as to not interfere with any methods
    /// defined on `T`. It should be called as `TrackedMut::downgrade(...)`.
    #[inline]
    pub fn downgrade(this: Self) -> Tracked<'a, T> {
        Tracked {
            value: this.value,
            constraint: this.constraint,
        }
    }

    /// Reborrow with a shorter lifetime.
    ///
    /// This is an associated function as to not interfere with any methods
    /// defined on `T`. It should be called as `TrackedMut::reborrow(...)`.
    #[inline]
    pub fn reborrow(this: &Self) -> Tracked<'_, T> {
        Tracked {
            value: this.value,
            constraint: this.constraint,
        }
    }

    /// Reborrow mutably with a shorter lifetime.
    ///
    /// This is an associated function as to not interfere with any methods
    /// defined on `T`. It should be called as `TrackedMut::reborrow_mut(...)`.
    #[inline]
    pub fn reborrow_mut(this: &mut Self) -> TrackedMut<'_, T> {
        TrackedMut {
            value: this.value,
            constraint: this.constraint,
        }
    }
}

impl<'a, T> Deref for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    type Target = <T::SurfaceMut as Family<'a>>::Out;

    #[inline]
    fn deref(&self) -> &Self::Target {
        T::surface_mut_ref(self)
    }
}

impl<'a, T> DerefMut for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        T::surface_mut_mut(self)
    }
}

impl<T> Debug for TrackedMut<'_, T>
where
    T: Track + ?Sized,
{
    #[inline]
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.pad("TrackedMut(..)")
    }
}

/// Destructure a `Tracked<_>` into its parts.
#[inline]
pub fn to_parts_ref<T>(tracked: Tracked<T>) -> (&T, Option<&Constraint<T::Call>>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}

/// Destructure a `TrackedMut<_>` into its parts.
#[inline]
pub fn to_parts_mut_ref<'a, T>(
    tracked: &'a TrackedMut<T>,
) -> (&'a T, Option<&'a Constraint<T::Call>>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}

/// Destructure a `TrackedMut<_>` into its parts.
#[inline]
pub fn to_parts_mut_mut<'a, T>(
    tracked: &'a mut TrackedMut<T>,
) -> (&'a mut T, Option<&'a Constraint<T::Call>>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}
