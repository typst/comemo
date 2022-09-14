use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::internal::Family;
use crate::track::{from_parts, to_parts, Track, Trackable, Tracked};

/// Ensure a type is suitable as input.
pub fn assert_hashable_or_trackable<T: Input>() {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
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

/// Type constructor for `'f -> Tracked<'f, T>`.
pub struct TrackedFamily<T>(PhantomData<T>);

impl<'f, T: Track + 'f> Family<'f> for TrackedFamily<T> {
    type Out = Tracked<'f, T>;
}

/// Wrapper for multiple inputs.
pub struct Args<T>(pub T);

/// Type constructor that maps a lifetime to tuple of arguments.
pub struct ArgsFamily<T>(PhantomData<T>);

macro_rules! args_input {
    ($($param:tt $idx:tt ),*) => {
        #[allow(unused_variables)]
        impl<$($param: Input),*> Input for Args<($($param,)*)> {
            type Constraint = ($($param::Constraint,)*);
            type Tracked = ArgsFamily<($($param,)*)>;

            fn key<T: Hasher>(&self, state: &mut T) {
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
        impl<'f, $($param: Input),*> Family<'f> for ArgsFamily<($($param,)*)> {
            type Out = ($(<$param::Tracked as Family<'f>>::Out,)*);
        }
    };
}

args_input! {}
args_input! { A 0 }
args_input! { A 0, B 1 }
args_input! { A 0, B 1, C 2 }
args_input! { A 0, B 1, C 2, D 3 }
args_input! { A 0, B 1, C 2, D 3, E 4 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5, G 6 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10 }
args_input! { A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11 }
