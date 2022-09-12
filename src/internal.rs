use std::cell::Cell;
use std::hash::Hash;
use std::marker::PhantomData;
use std::num::NonZeroU128;

use siphasher::sip128::{Hasher128, SipHasher};

use super::{Track, Tracked};

/// Destructure a tracker into its parts.
pub fn to_parts<'a, T>(tracked: Tracked<'a, T>) -> (&'a T, Option<&'a T::Tracker>)
where
    T: Track<'a>,
{
    (tracked.inner, tracked.tracker)
}

/// Create a tracker from its parts.
pub fn from_parts<'a, T>(inner: &'a T, tracker: Option<&'a T::Tracker>) -> Tracked<'a, T>
where
    T: Track<'a>,
{
    Tracked { inner, tracker }
}

/// Non-exposed parts of the `Track` trait.
pub trait Trackable<'a>: Sized + 'a {
    /// Keeps track of accesses to the value.
    type Tracker: Default;

    /// The tracked API surface of this type.
    type Surface;

    /// Whether an instance fulfills the given tracker's constraints.
    fn valid(&self, tracker: &Self::Tracker) -> bool;

    /// Cast a reference from `Tracked` to this type's surface.
    fn surface<'s>(tracked: &'s Tracked<'a, Self>) -> &'s Self::Surface
    where
        Self: Track<'a>;
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

/// Produce a non zero 128-bit hash of a value.
fn siphash<T: Hash>(value: &T) -> NonZeroU128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state
        .finish128()
        .as_u128()
        .try_into()
        .unwrap_or(NonZeroU128::new(u128::MAX).unwrap())
}
