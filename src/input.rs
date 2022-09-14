use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::internal::Family;
use crate::track::{from_parts, to_parts, Track, Trackable, Tracked};

/// An argument to a cached function.
pub trait Input {
    /// Describes an instance of the argument.
    type Constraint: Default + 'static;

    /// The argument with constraints hooked in.
    type Hooked: for<'f> Family<'f>;

    /// Hash the argument if it is a _key_ argument
    fn key<H: Hasher>(&self, state: &mut H);

    /// Validate the argument if it is a _tracked_ argument.
    fn valid(&self, constraint: &Self::Constraint) -> bool;

    /// Hook up the given constraints if this is a _tracked_ argument.
    fn hook_up<'f>(
        self,
        constraint: &'f Self::Constraint,
    ) -> <Self::Hooked as Family<'f>>::Out
    where
        Self: 'f;
}

impl<T: Hash> Input for T {
    /// No constraint for hashed arguments.
    type Constraint = ();

    /// The hooked-up type is just `Self`.
    type Hooked = IdFamily<Self>;

    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    fn valid(&self, _: &()) -> bool {
        true
    }

    fn hook_up<'f>(self, _: &'f ()) -> Self
    where
        Self: 'f,
    {
        self
    }
}

impl<'a, T: Track> Input for Tracked<'a, T> {
    /// Forwarded constraint from Trackable implementation.
    type Constraint = <T as Trackable>::Constraint;

    /// The hooked-up type is `Tracked<'f, T>`.
    type Hooked = TrackedFamily<T>;

    fn key<H: Hasher>(&self, _: &mut H) {}

    fn valid(&self, constraint: &Self::Constraint) -> bool {
        Trackable::valid(to_parts(*self).0, constraint)
    }

    fn hook_up<'f>(self, constraint: &'f Self::Constraint) -> Tracked<'f, T>
    where
        Self: 'f,
    {
        from_parts(to_parts(self).0, Some(constraint))
    }
}

/// Identity type constructor.
pub struct IdFamily<T>(PhantomData<T>);

impl<T> Family<'_> for IdFamily<T> {
    type Out = T;
}

/// 'f -> Tracked<'f, T> type constructor.
pub struct TrackedFamily<T>(PhantomData<T>);

impl<'f, T: Track + 'f> Family<'f> for TrackedFamily<T> {
    type Out = Tracked<'f, T>;
}
