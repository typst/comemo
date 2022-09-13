use std::cell::Cell;
use std::hash::Hash;
use std::marker::PhantomData;
use std::num::NonZeroU128;
use std::ops::Deref;

use crate::hash::siphash;

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
    T: Track + ?Sized,
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
pub fn to_parts<T>(tracked: Tracked<T>) -> (&T, Option<&T::Tracker>)
where
    T: Track,
{
    (tracked.inner, tracked.tracker)
}

/// Create a `Tracked<_>` from its parts.
pub fn from_parts<'a, T>(inner: &'a T, tracker: Option<&'a T::Tracker>) -> Tracked<'a, T>
where
    T: Track,
{
    Tracked { inner, tracker }
}

/// A trackable type.
///
/// This is implemented by types that have an implementation block annoted with
/// [`#[track]`](track).
pub trait Track: Trackable {
    /// Start tracking a value.
    fn track(&self) -> Tracked<Self> {
        Tracked { inner: self, tracker: None }
    }
}

/// Non-exposed parts of the `Track` trait.
pub trait Trackable {
    /// Keeps track of accesses to the value.
    type Tracker: Default + 'static;

    /// The tracked API surface of this type.
    type Surface: for<'a> Family<'a>;

    /// Whether an instance fulfills the given tracker's constraints.
    fn valid(&self, tracker: &Self::Tracker) -> bool;

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

/// Tracks accesses to a value.
#[derive(Default)]
pub struct AccessTracker<T: Hash>(Cell<Option<NonZeroU128>>, PhantomData<T>);

impl<T: Hash> AccessTracker<T> {
    pub fn track(&self, value: &T) {
        self.0.set(Some(siphash(value)));
    }

    pub fn valid(&self, value: &T) -> bool {
        self.0.get().map_or(true, |v| v == siphash(value))
    }
}
