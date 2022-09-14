use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::internal::Family;
use crate::track::{from_parts, to_parts, Track, Trackable, Tracked};

/// Ensure a type is suitable as input.
pub fn assert_hashable_or_trackable<T: Input>() {}

/// An input to a cached function.
pub trait Input {
    /// Describes an instance of this input.
    type Constraint: Default + 'static;

    /// The input with constraints hooked in.
    type Tracked: for<'f> Family<'f>;

    /// Hash the _key_ parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    /// Validate the _tracked_ parts of the input.
    fn valid(&self, constraint: &Self::Constraint) -> bool;

    /// Hook up the given constraint to the _tracked_ parts of the input.
    fn track<'f>(
        self,
        constraint: &'f Self::Constraint,
    ) -> <Self::Tracked as Family<'f>>::Out
    where
        Self: 'f;
}

impl<T: Hash> Input for T {
    /// No constraint for hashed inputs.
    type Constraint = ();

    /// The hooked-up type is just `Self`.
    type Tracked = IdFamily<Self>;

    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    fn valid(&self, _: &()) -> bool {
        true
    }

    fn track<'f>(self, _: &'f ()) -> Self
    where
        Self: 'f,
    {
        self
    }
}

/// Identity type constructor.
pub struct IdFamily<T>(PhantomData<T>);

impl<T> Family<'_> for IdFamily<T> {
    type Out = T;
}

impl<'a, T: Track> Input for Tracked<'a, T> {
    /// Forward constraint from `Trackable` implementation.
    type Constraint = <T as Trackable>::Constraint;

    /// The hooked-up type is `Tracked<'f, T>`.
    type Tracked = TrackedFamily<T>;

    fn key<H: Hasher>(&self, _: &mut H) {}

    fn valid(&self, constraint: &Self::Constraint) -> bool {
        Trackable::valid(to_parts(*self).0, constraint)
    }

    fn track<'f>(self, constraint: &'f Self::Constraint) -> Tracked<'f, T>
    where
        Self: 'f,
    {
        from_parts(to_parts(self).0, Some(constraint))
    }
}

/// 'f -> Tracked<'f, T> type constructor.
pub struct TrackedFamily<T>(PhantomData<T>);

impl<'f, T: Track + 'f> Family<'f> for TrackedFamily<T> {
    type Out = Tracked<'f, T>;
}

/// Wrapper for multiple inputs.
pub struct Args<T>(pub T);

/// Lifetime to tuple of arguments type constructor.
pub struct ArgsFamily<T>(PhantomData<T>);

macro_rules! args_input {
    ($($idx:tt: $letter:ident),*) => {
        #[allow(unused_variables)]
        impl<$($letter: Input),*> Input for Args<($($letter,)*)> {
            type Constraint = ($($letter::Constraint,)*);
            type Tracked = ArgsFamily<($($letter,)*)>;

            fn key<H: Hasher>(&self, state: &mut H) {
                $((self.0).$idx.key(state);)*
            }

            fn valid(&self, constraint: &Self::Constraint) -> bool {
                true $(&& (self.0).$idx.valid(&constraint.$idx))*
            }

            fn track<'f>(
                self,
                constraint: &'f Self::Constraint,
            ) -> <Self::Tracked as Family<'f>>::Out
            where
                Self: 'f,
            {
                ($((self.0).$idx.track(&constraint.$idx),)*)
            }
        }

        #[allow(unused_parens)]
        impl<'f, $($letter: Input),*> Family<'f> for ArgsFamily<($($letter,)*)> {
            type Out = ($(<$letter::Tracked as Family<'f>>::Out,)*);
        }
    };
}

args_input! {}
args_input! { 0: A }
args_input! { 0: A, 1: B }
args_input! { 0: A, 1: B, 2: C }
args_input! { 0: A, 1: B, 2: C, 3: D }
args_input! { 0: A, 1: B, 2: C, 3: D, 4: E }
args_input! { 0: A, 1: B, 2: C, 3: D, 4: E, 5: F }
args_input! { 0: A, 1: B, 2: C, 3: D, 4: E, 5: F, 6: G }
