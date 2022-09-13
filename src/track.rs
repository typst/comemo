use std::ops::Deref;

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
    inner: &'a T,
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
    T: Track,
{
    type Target = <T::Surface as Family<'a>>::Out;

    fn deref(&self) -> &Self::Target {
        T::surface(self)
    }
}

impl<'a, T> Copy for Tracked<'a, T> where T: Track {}

impl<'a, T> Clone for Tracked<'a, T>
where
    T: Track,
{
    fn clone(&self) -> Self {
        *self
    }
}

/// Destructure a `Tracked<_>` into its parts.
pub fn to_parts<T>(tracked: Tracked<T>) -> (&T, Option<&T::Constraint>)
where
    T: Track,
{
    (tracked.inner, tracked.constraint)
}

/// Create a `Tracked<_>` from its parts.
pub fn from_parts<'a, T>(
    inner: &'a T,
    constraint: Option<&'a T::Constraint>,
) -> Tracked<'a, T>
where
    T: Track,
{
    Tracked { inner, constraint }
}

/// A trackable type.
///
/// This is implemented by types that have an implementation block annoted with
/// [`#[track]`](macro@crate::track).
pub trait Track: Trackable {
    /// Start tracking a value.
    fn track(&self) -> Tracked<Self> {
        Tracked { inner: self, constraint: None }
    }
}

/// Non-exposed parts of the `Track` trait.
pub trait Trackable {
    /// Describes an instance of type.
    type Constraint: Default + 'static;

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

/// Workaround for Surface<'a> until GATs are stable.
pub trait Family<'a> {
    /// The surface with lifetime.
    type Out;
}
