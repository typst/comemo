use std::fmt::{self, Debug, Formatter};
use std::ops::{Deref, DerefMut};

use crate::accelerate;
use crate::constraint::Join;

/// A trackable type.
///
/// This is implemented by types that have an implementation block annotated
/// with `#[track]` and for trait objects whose traits are annotated with
/// `#[track]`. For more details, see [its documentation](macro@crate::track).
pub trait Track: Validate + Surfaces {
    /// Start tracking all accesses to a value.
    #[inline]
    fn track(&self) -> Tracked<Self> {
        Tracked {
            value: self,
            constraint: None,
            id: accelerate::id(),
        }
    }

    /// Start tracking all accesses and mutations to a value.
    #[inline]
    fn track_mut(&mut self) -> TrackedMut<Self> {
        TrackedMut { value: self, constraint: None }
    }

    /// Start tracking all accesses into a constraint.
    #[inline]
    fn track_with<'a>(&'a self, constraint: &'a Self::Constraint) -> Tracked<'a, Self> {
        Tracked {
            value: self,
            constraint: Some(constraint),
            id: accelerate::id(),
        }
    }

    /// Start tracking all accesses and mutations into a constraint.
    #[inline]
    fn track_mut_with<'a>(
        &'a mut self,
        constraint: &'a Self::Constraint,
    ) -> TrackedMut<'a, Self> {
        TrackedMut { value: self, constraint: Some(constraint) }
    }
}

/// A type that can be validated against constraints.
///
/// Typical crate usage does not require you to interact with its trait.
/// However, it can be useful if you want to integrate comemo's tracking and
/// constraint validation mechanism directly into your code.
///
/// This trait is implemented by the `#[track]` macro alongside [`Track`].
pub trait Validate {
    /// The constraints for this type.
    type Constraint: Default + Clone + Join + 'static;

    /// Whether this value fulfills the given constraints.
    ///
    /// For a type `Foo`, empty constraints can be created with `<Foo as
    /// Validate>::Constraint::default()` and filled with
    /// [`track_with`](Track::track_with) or
    /// [`track_mut_with`](Track::track_mut_with).
    fn validate(&self, constraint: &Self::Constraint) -> bool;

    /// Accelerated version of [`validate`](Self::validate).
    ///
    /// A `id` uniquely identifies a value to speed up repeated validation of
    /// equal constraints against the same value. If given the same `id` twice,
    /// `self` must also be identical, unless [`evict`](crate::evict) has been
    /// called in between.
    fn validate_with_id(&self, constraint: &Self::Constraint, id: usize) -> bool;

    /// Replay recorded mutations to the value.
    fn replay(&mut self, constraint: &Self::Constraint);
}

/// This type's tracked surfaces.
pub trait Surfaces {
    /// The tracked API surface of this type.
    type Surface<'a>
    where
        Self: 'a;

    /// The mutable tracked API surface of this type.
    type SurfaceMut<'a>
    where
        Self: 'a;

    /// Access the immutable surface from a `Tracked`.
    fn surface_ref<'a, 't>(tracked: &'t Tracked<'a, Self>) -> &'t Self::Surface<'a>
    where
        Self: Track;

    /// Access the immutable surface from a `TrackedMut`.
    fn surface_mut_ref<'a, 't>(
        tracked: &'t TrackedMut<'a, Self>,
    ) -> &'t Self::SurfaceMut<'a>
    where
        Self: Track;

    /// Access the mutable surface from a `TrackedMut`.
    fn surface_mut_mut<'a, 't>(
        tracked: &'t mut TrackedMut<'a, Self>,
    ) -> &'t mut Self::SurfaceMut<'a>
    where
        Self: Track;
}

/// Tracks accesses to a value.
///
/// Encapsulates a reference to a value and tracks all accesses to it. The only
/// methods accessible on `Tracked<T>` are those defined in an implementation
/// block or trait for `T` annotated with `#[track]`. For more details, see [its
/// documentation](macro@crate::track).
///
/// ## Variance
/// Typically you can ignore the defaulted `C` parameter. However, due to
/// compiler limitations, this type will then be invariant over `T`. This limits
/// how it can be used. In particular, invariance prevents you from creating a
/// usable _chain_ of tracked types.
///
/// ```ignore
/// struct Chain<'a> {
///     outer: Tracked<'a, Self>,
///     data: u32, // some data for the chain link
/// }
/// ```
///
/// However, this is sometimes a useful pattern (for example, it allows you to
/// detect cycles in memoized recursive algorithms). If you want to create a
/// tracked chain or need covariance for another reason, you need to manually
/// specify the constraint type like so:
///
/// ```ignore
/// struct Chain<'a> {
///     outer: Tracked<'a, Self, <Chain<'static> as Validate>::Constraint>,
///     data: u32, // some data for the chain link
/// }
/// ```
///
/// Notice the `'static` lifetime: This makes the compiler understand that no
/// strange business that depends on `'a` is happening in the associated
/// constraint type. (In fact, all constraints are `'static`.)
pub struct Tracked<'a, T, C = <T as Validate>::Constraint>
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
    pub(crate) constraint: Option<&'a C>,
    /// A unique ID for validation acceleration.
    pub(crate) id: usize,
}

// The type `Tracked<T>` automatically dereferences to T's generated surface
// type. This makes all tracked methods available, but leaves all other ones
// unaccessible.
impl<'a, T> Deref for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    type Target = T::Surface<'a>;

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
///
/// For more details, see [`Tracked`].
pub struct TrackedMut<'a, T, C = <T as Validate>::Constraint>
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
    pub(crate) constraint: Option<&'a C>,
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
            id: accelerate::id(),
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
            id: accelerate::id(),
        }
    }

    /// Reborrow mutably with a shorter lifetime.
    ///
    /// This is an associated function as to not interfere with any methods
    /// defined on `T`. It should be called as `TrackedMut::reborrow_mut(...)`.
    #[inline]
    pub fn reborrow_mut(this: &mut Self) -> TrackedMut<'_, T> {
        TrackedMut { value: this.value, constraint: this.constraint }
    }
}

impl<'a, T> Deref for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    type Target = T::SurfaceMut<'a>;

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
pub fn to_parts_ref<T>(tracked: Tracked<T>) -> (&T, Option<&T::Constraint>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}

/// Destructure a `TrackedMut<_>` into its parts.
#[inline]
pub fn to_parts_mut_ref<'a, T>(
    tracked: &'a TrackedMut<T>,
) -> (&'a T, Option<&'a T::Constraint>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}

/// Destructure a `TrackedMut<_>` into its parts.
#[inline]
pub fn to_parts_mut_mut<'a, T>(
    tracked: &'a mut TrackedMut<T>,
) -> (&'a mut T, Option<&'a T::Constraint>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.constraint)
}
