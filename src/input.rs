use std::hash::{Hash, Hasher};

use crate::constraint::Join;
use crate::track::{Track, Tracked, TrackedMut, Validate};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<'a, In: Input<'a>>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input<'a> {
    /// The constraints for this input.
    type Constraint: Default + Clone + Join + 'static;

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
    fn retrack(self, constraint: &'a Self::Constraint) -> (Self, Self::Outer)
    where
        Self: Sized;
}

impl<'a, T: Hash> Input<'a> for T {
    // No constraint for hashed inputs.
    type Constraint = ();
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
    fn retrack(self, _: &'a ()) -> (Self, Self::Outer) {
        (self, ())
    }
}

impl<'a, T> Input<'a> for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = <T as Validate>::Constraint;
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
    fn retrack(self, constraint: &'a Self::Constraint) -> (Self, Self::Outer) {
        let tracked = Tracked {
            value: self.value,
            constraint: Some(constraint),
            id: self.id,
        };
        (tracked, self.constraint)
    }
}

impl<'a, T> Input<'a> for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = T::Constraint;
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
    fn retrack(self, constraint: &'a Self::Constraint) -> (Self, Self::Outer) {
        let tracked = TrackedMut { value: self.value, constraint: Some(constraint) };
        (tracked, self.constraint)
    }
}

/// Wrapper for multiple inputs.
pub struct Multi<T>(pub T);

macro_rules! multi {
    ($($param:tt $alt:tt $idx:tt ),*) => {
        #[allow(unused_variables, non_snake_case)]
        impl<'a, $($param: Input<'a>),*> Input<'a> for Multi<($($param,)*)> {
            type Constraint = ($($param::Constraint,)*);
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
            fn retrack(
                self,
                constraint: &'a Self::Constraint,
            ) -> (Self, Self::Outer) {
                $(let $param = (self.0).$idx.retrack(&constraint.$idx);)*
                (Multi(($($param.0,)*)), ($($param.1,)*))
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

multi! {}
multi! { A Z 0 }
multi! { A Z 0, B Y 1 }
multi! { A Z 0, B Y 1, C X 2 }
multi! { A Z 0, B Y 1, C X 2, D W 3 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9, K P 10 }
multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9, K P 10, L O 11 }
