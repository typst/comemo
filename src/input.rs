use std::hash::{Hash, Hasher};

use bumpalo::Bump;

use crate::constraint::Call;
use crate::track::{Track, Tracked, TrackedMut};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<In: Input>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input {
    type Call: Call;

    /// The input with new constraints hooked in.
    type Tracked<'r>
    where
        Self: 'r;

    /// Hash the key parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    fn call(&self, call: Self::Call) -> u128;

    fn call_mut(&mut self, call: Self::Call) -> u128;

    /// Hook up the given constraint to the tracked parts of the input and
    /// return the result alongside the outer constraints.
    fn retrack<'r>(
        self,
        sink: impl Fn(Self::Call, u128) + Copy + Send + Sync + 'r,
        b: &'r Bump,
    ) -> Self::Tracked<'r>
    where
        Self: 'r;

    fn retrack_noop<'r>(self) -> Self::Tracked<'r>
    where
        Self: 'r;
}

impl<T: Hash> Input for T {
    // No constraint for hashed inputs.
    type Call = ();
    type Tracked<'r>
        = Self
    where
        Self: 'r;

    #[inline]
    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    #[inline]
    fn call(&self, _: Self::Call) -> u128 {
        0
    }

    #[inline]
    fn call_mut(&mut self, _: Self::Call) -> u128 {
        0
    }

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

    fn retrack_noop<'r>(self) -> Self::Tracked<'r>
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
    type Call = T::Call;
    type Tracked<'r>
        = Tracked<'r, T>
    where
        Self: 'r;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: Self::Call) -> u128 {
        // if let Some(accelerator) = crate::accelerate::get(self.id) {
        //     let mut map = accelerator.lock();
        //     let call_hash = crate::constraint::hash(&call);
        //     return *map.entry(call_hash).or_insert_with(|| self.value.call(call));
        // }
        let hash = self.value.call(call.clone());
        if let Some(sink) = self.sink {
            sink(call, hash)
        }
        hash
    }

    #[inline]
    fn call_mut(&mut self, call: Self::Call) -> u128 {
        0
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

    fn retrack_noop<'r>(self) -> Self::Tracked<'r>
    where
        Self: 'r,
    {
        self
    }
}

impl<'a, T> Input for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Call = T::Call;
    type Tracked<'r>
        = TrackedMut<'r, T>
    where
        Self: 'r;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: Self::Call) -> u128 {
        let hash = self.value.call(call.clone());
        if let Some(sink) = self.sink {
            sink(call, hash)
        }
        hash
    }

    #[inline]
    fn call_mut(&mut self, call: Self::Call) -> u128 {
        let hash = self.value.call_mut(call.clone());
        if let Some(sink) = self.sink {
            sink(call, hash)
        }
        hash
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

    fn retrack_noop<'r>(self) -> Self::Tracked<'r>
    where
        Self: 'r,
    {
        self
    }
}

/// Wrapper for multiple inputs.
pub struct Args<T>(pub T);

macro_rules! args_input {
    ($($param:tt $alt:tt $idx:tt),*) => {
        #[allow(unused_variables, clippy::unused_unit, non_snake_case)]
        const _: () = {
            impl<$($param: Input),*> Input for Args<($($param,)*)> {
                type Call = ArgsCall<$($param::Call),*>;
                type Tracked<'r> = ($($param::Tracked<'r>,)*) where Self: 'r;

                #[inline]
                fn key<T: Hasher>(&self, state: &mut T) {
                    $((self.0).$idx.key(state);)*
                }

                #[inline]
                fn call(&self, call: Self::Call) -> u128 {
                    match call {
                        $(ArgsCall::$param($param) => (self.0).$idx.call($param)),*
                    }
                }

                #[inline]
                fn call_mut(&mut self, call: Self::Call) -> u128 {
                    match call {
                        $(ArgsCall::$param($param) => (self.0).$idx.call_mut($param)),*
                    }
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
                        move |call, hash| sink(ArgsCall::$param(call), hash),
                        bump,
                    );)*
                    ($($param,)*)
                }

                fn retrack_noop<'r>(self) -> Self::Tracked<'r>
                where
                    Self: 'r,
                {
                    $(let $param = (self.0).$idx.retrack_noop();)*
                    ($($param,)*)
                }
            }

            #[derive(PartialEq, Clone, Hash)]
            pub enum ArgsCall<$($param),*> {
                $($param($param),)*
            }

            impl<$($param: Call),*> Call for ArgsCall<$($param),*> {
                fn is_mutable(&self) -> bool {
                    match *self {
                        $(Self::$param(ref $param) => $param.is_mutable(),)*
                    }
                }
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
