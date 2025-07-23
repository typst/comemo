use std::hash::{Hash, Hasher};

use bumpalo::Bump;

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

    type Call: Clone + Send + Sync;

    /// The input with new constraints hooked in.
    type Tracked<'r>
    where
        Self: 'r;

    /// The extracted outer constraints.
    type Outer: Join<Self::Constraint>;

    /// Hash the key parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    fn call(&self, call: Self::Call) -> u128;

    /// Validate the tracked parts of the input.
    fn validate(&self, constraint: &Self::Constraint) -> bool;

    /// Replay mutations to the input.
    fn replay(&mut self, constraint: &Self::Constraint);

    /// Hook up the given constraint to the tracked parts of the input and
    /// return the result alongside the outer constraints.
    fn retrack<'r>(
        self,
        sink: impl Fn(Self::Call, u128) + Copy + Send + Sync + 'r,
        b: &'r Bump,
    ) -> Self::Tracked<'r>
    where
        Self: 'r;
}

impl<T: Hash> Input for T {
    // No constraint for hashed inputs.
    type Constraint = ();
    type Call = ();
    type Tracked<'r>
        = Self
    where
        Self: 'r;
    type Outer = ();

    #[inline]
    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    #[inline]
    fn call(&self, _: Self::Call) -> u128 {
        0
    }

    #[inline]
    fn validate(&self, _: &()) -> bool {
        true
    }

    #[inline]
    fn replay(&mut self, _: &Self::Constraint) {}

    #[inline]
    fn retrack<'r>(
        self,
        _: impl Fn(Self::Call, u128) + Copy + Send + Sync + 'r,
        _: &'r Bump,
    ) -> Self::Tracked<'r>
    where
        Self: 'r,
    {
        self
    }
}

impl<'a, T> Input for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = <T as Validate>::Constraint;
    type Call = T::Call;
    type Tracked<'r>
        = Tracked<'r, T>
    where
        Self: 'r;
    type Outer = Option<&'a Self::Constraint>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: Self::Call) -> u128 {
        self.value.call(call)
    }

    #[inline]
    fn validate(&self, constraint: &Self::Constraint) -> bool {
        self.value.validate_with_id(constraint, self.id)
    }

    #[inline]
    fn replay(&mut self, _: &Self::Constraint) {}

    #[inline]
    fn retrack<'r>(
        self,
        sink: impl Fn(Self::Call, u128) + Copy + Send + Sync + 'r,
        b: &'r Bump,
    ) -> Self::Tracked<'r>
    where
        Self: 'r,
    {
        let prev = self.sink;
        Tracked {
            value: self.value,
            id: self.id,
            sink: Some(b.alloc(move |c: T::Call, hash: u128| {
                sink(c.clone(), hash);
                if let Some(prev) = prev {
                    prev(c, hash);
                }
            })),
        }
    }
}

impl<'a, T> Input for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Constraint = T::Constraint;
    type Call = T::Call;
    type Tracked<'r>
        = TrackedMut<'r, T>
    where
        Self: 'r;
    type Outer = Option<&'a Self::Constraint>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: Self::Call) -> u128 {
        self.value.call(call)
    }

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
        sink: impl Fn(Self::Call, u128) + Copy + Send + Sync + 'r,
        b: &'r Bump,
    ) -> Self::Tracked<'r>
    where
        Self: 'r,
    {
        let prev = self.sink;
        TrackedMut {
            value: self.value,
            sink: Some(b.alloc(move |c: T::Call, hash: u128| {
                sink(c.clone(), hash);
                if let Some(prev) = prev {
                    prev(c, hash);
                }
            })),
        }
    }
}

/// Wrapper for multiple inputs.
pub struct Args<T>(pub T);

macro_rules! args_input {
    ($($param:tt $alt:tt $idx:tt),*) => {
        const _: () = {
            #[allow(unused_variables, clippy::unused_unit, non_snake_case)]
            impl<$($param: Input),*> Input for Args<($($param,)*)> {
                type Constraint = ($($param::Constraint,)*);
                type Call = Call<$($param::Call),*>;
                type Tracked<'r> = ($($param::Tracked<'r>,)*) where Self: 'r;
                type Outer = ($($param::Outer,)*);

                #[inline]
                fn key<T: Hasher>(&self, state: &mut T) {
                    $((self.0).$idx.key(state);)*
                }

                #[inline]
                fn call(&self, call: Self::Call) -> u128 {
                    match call {
                        $(Call::$param($param) => (self.0).$idx.call($param)),*
                    }
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
                    sink: impl Fn(Self::Call, u128) + Copy + Send + Sync + 'r,
                    bump: &'r Bump,
                ) -> Self::Tracked<'r>
                where
                    Self: 'r,
                {
                    $(let $param = (self.0).$idx.retrack(
                        move |call, hash| sink(Call::$param(call), hash),
                        bump,
                    );)*
                    ($($param,)*)
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

            #[derive(Clone)]
            pub enum Call<$($param),*> {
                $($param($param),)*
            }
        };
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
