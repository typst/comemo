use std::fmt::{self, Debug, Formatter};
use std::ops::{Deref, DerefMut};

use crate::accelerate;
use crate::call::Call;

/// A sink to which tracked calls can be sent.
pub trait Sink<C>: Send + Sync {
    /// Emit a call and its return hash to the sink.
    ///
    /// Returns whether the call was handled. If `false`, the call was
    /// deduplicated.
    fn emit(&self, call: C, ret: u128) -> bool;
}

impl<C: Call> Sink<C> for () {
    fn emit(&self, _: C, _: u128) -> bool {
        true
    }
}

/// A trackable type.
///
/// This is implemented by types that have an implementation block annotated
/// with `#[track]` and for trait objects whose traits are annotated with
/// `#[track]`. For more details, see [its documentation](macro@crate::track).
pub trait Track: Surfaces {
    /// An enumeration of possible tracked calls that can be performed on this
    /// tracked type.
    type Call: Call;

    /// Performs a call on the value and returns the hash of its results.
    fn call(&self, call: Self::Call) -> u128;

    /// Performs a mutable call on the value.
    fn call_mut(&mut self, call: Self::Call) -> u128;

    /// Start tracking all accesses to a value.
    #[inline]
    fn track(&self) -> Tracked<'_, Self> {
        Tracked { value: self, sink: None, id: accelerate::id() }
    }

    /// Start tracking all accesses and mutations to a value.
    #[inline]
    fn track_mut(&mut self) -> TrackedMut<'_, Self> {
        TrackedMut { value: self, sink: None }
    }

    /// Start tracking all accesses into a sink.
    #[inline]
    fn track_with<'a>(&'a self, sink: &'a dyn Sink<Self::Call>) -> Tracked<'a, Self> {
        Tracked {
            value: self,
            sink: Some(sink),
            id: accelerate::id(),
        }
    }

    /// Start tracking all accesses and mutations into a sink.
    #[inline]
    fn track_mut_with<'a>(
        &'a mut self,
        sink: &'a dyn Sink<Self::Call>,
    ) -> TrackedMut<'a, Self> {
        TrackedMut { value: self, sink: Some(sink) }
    }
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
/// ```
/// # use comemo::{Track, Tracked};
/// struct Chain<'a> {
///     outer: Tracked<'a, Self>,
///     data: u32, // some data for the chain link
/// }
/// # #[comemo::track] impl<'a> Chain<'a> {}
/// ```
///
/// However, this is sometimes a useful pattern (for example, it allows you to
/// detect cycles in memoized recursive algorithms). If you want to create a
/// tracked chain or need covariance for another reason, you need to manually
/// specify the call type like so:
///
/// ```
/// # use comemo::{Track, Tracked};
/// struct Chain<'a> {
///     outer: Tracked<'a, Self, <Chain<'static> as Track>::Call>,
///     data: u32, // some data for the chain link
/// }
/// # #[comemo::track] impl<'a> Chain<'a> {}
/// ```
///
/// Notice the `'static` lifetime: This makes the compiler understand that no
/// strange business that depends on `'a` is happening in the associated
/// constraint type. (In fact, all constraints are `'static`.)
pub struct Tracked<'a, T, C = <T as Track>::Call>
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
    pub(crate) sink: Option<&'a dyn Sink<C>>,
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
pub struct TrackedMut<'a, T, C = <T as Track>::Call>
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
    pub(crate) sink: Option<&'a dyn Sink<C>>,
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
            sink: this.sink,
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
            sink: this.sink,
            id: accelerate::id(),
        }
    }

    /// Reborrow mutably with a shorter lifetime.
    ///
    /// This is an associated function as to not interfere with any methods
    /// defined on `T`. It should be called as `TrackedMut::reborrow_mut(...)`.
    #[inline]
    pub fn reborrow_mut(this: &mut Self) -> TrackedMut<'_, T> {
        TrackedMut { value: this.value, sink: this.sink }
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
pub fn to_parts_ref<T>(tracked: Tracked<'_, T>) -> (&T, Option<&dyn Sink<T::Call>>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.sink)
}

/// Destructure a `TrackedMut<_>` into its parts.
#[inline]
pub fn to_parts_mut_ref<'a, T>(
    tracked: &'a TrackedMut<T>,
) -> (&'a T, Option<&'a dyn Sink<T::Call>>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.sink)
}

/// Destructure a `TrackedMut<_>` into its parts.
#[inline]
pub fn to_parts_mut_mut<'a, T>(
    tracked: &'a mut TrackedMut<T>,
) -> (&'a mut T, Option<&'a dyn Sink<T::Call>>)
where
    T: Track + ?Sized,
{
    (tracked.value, tracked.sink)
}
