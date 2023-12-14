use std::hash::{Hash, Hasher};

use crate::constraint::Join;
use crate::track::{Track, Tracked, TrackedMut, Validate};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<In: Input>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input {
    /// The constraints for this input.
    type Constraint: Default + Clone + Join + 'static;

    /// The input with new constraints hooked in.
    type Tracked<'r>
    where
        Self: 'r;

    /// The extracted outer constraints.
    type Outer: Join<Self::Constraint>;

    /// Hash the key parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    /// Validate the tracked parts of the input.
    fn validate(&self, constraint: &Self::Constraint) -> bool;

    /// Replay mutations to the input.
    fn replay(&mut self, constraint: &Self::Constraint);

    /// Hook up the given constraint to the tracked parts of the input and
    /// return the result alongside the outer constraints.
    fn retrack<'r>(
        self,
        constraint: &'r Self::Constraint,
    ) -> (Self::Tracked<'r>, Self::Outer)
    where
        Self: 'r;
}

impl<T: Hash> Input for T {
    // No constraint for hashed inputs.
    type Constraint = ();
    type Tracked<'r> = Self where Self: 'r;
    type Outer = ();

    #[inline]
    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    #[inline]
    fn validate(&self, _: &()) -> bool {
        true
    }

    #[inline]
    fn replay(&mut self, _: &Self::Constraint) {}

    #[inline]
    fn retrack<'r>(self, _: &'r ()) -> (Self::Tracked<'r>, Self::Outer)
    where
        Self: 'r,
    {
        (self, ())
    }
}

impl<'a, T> Input for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = <T as Validate>::Constraint;
    type Tracked<'r> = Tracked<'r, T> where Self: 'r;
    type Outer = Option<&'a Self::Constraint>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn validate(&self, constraint: &Self::Constraint) -> bool {
        self.value.validate_with_id(constraint, self.id)
    }

    #[inline]
    fn replay(&mut self, _: &Self::Constraint) {}

    #[inline]
    fn retrack<'r>(
        self,
        constraint: &'r Self::Constraint,
    ) -> (Self::Tracked<'r>, Self::Outer)
    where
        Self: 'r,
    {
        let tracked = Tracked {
            value: self.value,
            constraint: Some(constraint),
            id: self.id,
        };
        (tracked, self.constraint)
    }
}

impl<'a, T> Input for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = T::Constraint;
    type Tracked<'r> = TrackedMut<'r, T> where Self: 'r;
    type Outer = Option<&'a Self::Constraint>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn validate(&self, constraint: &Self::Constraint) -> bool {
        self.value.validate(constraint)
    }

    #[inline]
    fn replay(&mut self, constraint: &Self::Constraint) {
        self.value.replay(constraint);
    }

    #[inline]
    fn retrack<'r>(
        self,
        constraint: &'r Self::Constraint,
    ) -> (Self::Tracked<'r>, Self::Outer)
    where
        Self: 'r,
    {
        let tracked = TrackedMut { value: self.value, constraint: Some(constraint) };
        (tracked, self.constraint)
    }
}

/// Wrapper for multiple inputs.
pub struct Args<T>(pub T);

macro_rules! args_input {
    ($($param:tt $alt:tt $idx:tt ),*) => {
        #[allow(unused_variables, non_snake_case)]
        impl<$($param: Input),*> Input for Args<($($param,)*)> {
            type Constraint = ($($param::Constraint,)*);
            type Tracked<'r> = ($($param::Tracked<'r>,)*) where Self: 'r;
            type Outer = ($($param::Outer,)*);

            #[inline]
            fn key<T: Hasher>(&self, state: &mut T) {
                $((self.0).$idx.key(state);)*
            }

            #[inline]
            fn validate(&self, constraint: &Self::Constraint) -> bool {
                true $(&& (self.0).$idx.validate(&constraint.$idx))*
            }

            #[inline]
            fn replay(&mut self, constraint: &Self::Constraint) {
                $((self.0).$idx.replay(&constraint.$idx);)*
            }

            #[inline]
            fn retrack<'r>(
                self,
                constraint: &'r Self::Constraint,
            ) -> (Self::Tracked<'r>, Self::Outer)
            where
                Self: 'r,
            {
                $(let $param = (self.0).$idx.retrack(&constraint.$idx);)*
                (($($param.0,)*), ($($param.1,)*))
            }
        }

        #[allow(unused_variables, clippy::unused_unit)]
        impl<$($param: Join<$alt>, $alt),*> Join<($($alt,)*)> for ($($param,)*) {
            #[inline]
            fn join(&self, constraint: &($($alt,)*)) {
                $(self.$idx.join(&constraint.$idx);)*
            }

            #[inline]
            fn take(&self) -> Self {
                ($(self.$idx.take(),)*)
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
