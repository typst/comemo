use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use crate::constraint::Join;
use crate::internal::Family;
use crate::track::{from_parts, to_parts, Track, Trackable, Tracked};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<In: Input>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input {
    /// Describes an instance of this input.
    type Constraint: Default + Debug + Join + 'static;

    /// The input with new constraints hooked in.
    type Tracked: for<'f> Family<'f>;

    /// The extracted outer constraints.
    type Outer: Join<Self::Constraint>;

    /// Hash the key parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    /// Validate the tracked parts of the input.
    fn valid(&self, constraint: &Self::Constraint) -> bool;

    /// Hook up the given constraint to the tracked parts of the input and
    /// return the result alongside the outer constraints.
    fn retrack<'f>(
        self,
        constraint: &'f Self::Constraint,
    ) -> (<Self::Tracked as Family<'f>>::Out, Self::Outer)
    where
        Self: 'f;
}

impl<T> Input for T
where
    T: Hash,
{
    // No constraint for hashed inputs.
    type Constraint = ();
    type Tracked = IdFamily<Self>;
    type Outer = ();

    #[inline]
    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    #[inline]
    fn valid(&self, _: &()) -> bool {
        true
    }

    #[inline]
    fn retrack<'f>(self, _: &'f ()) -> (Self, ())
    where
        Self: 'f,
    {
        (self, ())
    }
}

/// Identity type constructor.
pub struct IdFamily<T>(PhantomData<T>);

impl<T> Family<'_> for IdFamily<T> {
    type Out = T;
}

impl<'a, T> Input for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = T::Constraint;
    type Tracked = TrackedFamily<T>;
    type Outer = Option<&'a T::Constraint>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn valid(&self, constraint: &Self::Constraint) -> bool {
        Trackable::valid(to_parts(*self).0, constraint)
    }

    #[inline]
    fn retrack<'f>(
        self,
        constraint: &'f Self::Constraint,
    ) -> (Tracked<'f, T>, Option<&'a T::Constraint>)
    where
        Self: 'f,
    {
        let (value, outer) = to_parts(self);
        (from_parts(value, Some(constraint)), outer)
    }
}

/// Type constructor for `'f -> Tracked<'f, T>`.
pub struct TrackedFamily<T: ?Sized>(PhantomData<T>);

impl<'f, T> Family<'f> for TrackedFamily<T>
where
    T: Track + ?Sized + 'f,
{
    type Out = Tracked<'f, T>;
}

/// Wrapper for multiple inputs.
pub struct Args<T>(pub T);

/// Type constructor that maps a lifetime to tuple of arguments.
pub struct ArgsFamily<T>(PhantomData<T>);

macro_rules! args_input {
    ($($param:tt $alt:tt $idx:tt ),*) => {
        #[allow(unused_variables)]
        impl<$($param: Input),*> Input for Args<($($param,)*)> {
            type Constraint = ($($param::Constraint,)*);
            type Tracked = ArgsFamily<($($param,)*)>;
            type Outer = ($($param::Outer,)*);

            #[inline]
            fn key<T: Hasher>(&self, state: &mut T) {
                $((self.0).$idx.key(state);)*
            }

            #[inline]
            fn valid(&self, constraint: &Self::Constraint) -> bool {
                true $(&& (self.0).$idx.valid(&constraint.$idx))*
            }

            #[allow(non_snake_case)]
            #[inline]
            fn retrack<'f>(
                            self,
                constraint: &'f Self::Constraint,
            ) -> (<Self::Tracked as Family<'f>>::Out, Self::Outer)
            where
                Self: 'f,
            {
                $(let $param = (self.0).$idx.retrack(&constraint.$idx);)*
                (($($param.0,)*), ($($param.1,)*))
            }
        }

        #[allow(unused_parens)]
        impl<'f, $($param: Input),*> Family<'f> for ArgsFamily<($($param,)*)> {
            type Out = ($(<$param::Tracked as Family<'f>>::Out,)*);
        }

        #[allow(unused_variables)]
        impl<$($param: Join<$alt>, $alt),*> Join<($($alt,)*)> for ($($param,)*) {
            #[inline]
            fn join(&self, constraint: &($($alt,)*)) {
                $(self.$idx.join(&constraint.$idx);)*
            }
        }
    };
}

args_input! {}
args_input! { A Z 0 }
args_input! { A Z 0, B Y 1 }
args_input! { A Z 0, B Y 1, C X 2 }
args_input! { A Z 0, B Y 1, C X 2, D W 3 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9, K P 10 }
args_input! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9, K P 10, L O 11 }
