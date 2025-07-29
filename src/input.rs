use std::hash::{Hash, Hasher};

use bumpalo::Bump;

use crate::call::Call;
use crate::track::{Sink, Track, Tracked, TrackedMut};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<'c, In: Input<'c>>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input<'a> {
    /// An enumeration of possible tracked calls that can be performed on any
    /// tracked part of this input.
    type Call: Call;
    type Sink;

    /// Hashes the key (i.e. not tracked) parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    /// Performs a call on a tracked part of the input and returns the hash of
    /// the result. If the call is mutable, the side effect will not be
    /// observable.
    fn call(&self, call: Self::Call) -> u128;

    /// Perform a call on a tracked part of the input and returns the hash of
    /// the result. If the call is mutable, the side effect will be
    /// observable.
    fn call_mut(&mut self, call: Self::Call) -> u128;

    /// Hook up the given constraint to the tracked parts of the input and
    /// return the result alongside the outer constraints.
    fn retrack(
        &mut self,
        sink: impl Fn(Self::Call, u128) -> bool + Copy + Send + Sync + 'a,
        b: &'a Bump,
    );
}

impl<'a, T: Hash> Input<'a> for T {
    // No constraint for hashed inputs.
    type Call = ();
    type Sink = ();

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
    fn retrack(
        &mut self,
        _: impl Fn(Self::Call, u128) -> bool + Copy + Send + Sync + 'a,
        _: &'a Bump,
    ) {
    }
}

impl<'a, T> Input<'a> for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Call = T::Call;
    type Sink = ();

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: Self::Call) -> u128 {
        let hash = if let Some(accelerator) = crate::accelerate::get(self.id) {
            let mut map = accelerator.lock();
            let call_hash = crate::hash::hash(&call);
            *map.entry(call_hash).or_insert_with(|| self.value.call(call.clone()))
        } else {
            self.value.call(call.clone())
        };
        if let Some(sink) = self.sink {
            sink.emit(call, hash);
        }
        hash
    }

    #[inline]
    fn call_mut(&mut self, _: Self::Call) -> u128 {
        // Cannot perform a mutable call on an immutable reference.
        0
    }

    #[inline]
    fn retrack(
        &mut self,
        sink: impl Fn(Self::Call, u128) -> bool + Copy + Send + Sync + 'a,
        b: &'a Bump,
    ) {
        // let prev = self.sink;
        // self.sink = Some(b.alloc(move |c: T::Call, hash: u128| {
        //     if sink(c.clone(), hash)
        //         && let Some(prev) = prev
        //     {
        //         prev(c, hash);
        //     }
        // }));
    }
}

impl<'a, T> Input<'a> for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Call = T::Call;
    type Sink = ();

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: Self::Call) -> u128 {
        let hash = self.value.call(call.clone());
        if let Some(sink) = self.sink {
            sink.emit(call, hash);
        }
        hash
    }

    #[inline]
    fn call_mut(&mut self, call: Self::Call) -> u128 {
        let hash = self.value.call_mut(call.clone());
        if let Some(sink) = self.sink {
            sink.emit(call, hash);
        }
        hash
    }

    #[inline]
    fn retrack(
        &mut self,
        sink: impl Fn(Self::Call, u128) -> bool + Copy + Send + Sync + 'a,
        b: &'a Bump,
    ) {
        // let prev = self.sink;
        // self.sink = Some(b.alloc(move |c: T::Call, hash: u128| {
        //     if sink(c.clone(), hash)
        //         && let Some(prev) = prev
        //     {
        //         prev(c, hash);
        //     }
        // }));
    }
}

/// Wrapper for multiple inputs.
pub struct Multi<T>(pub T);

macro_rules! multi {
    ($($param:tt $alt:tt $idx:tt),*) => {
        #[allow(unused_variables, clippy::unused_unit, non_snake_case)]
        const _: () = {
            impl<'a, $($param: Input<'a>),*> Input<'a> for Multi<($($param,)*)> {
                type Call = MultiCall<$($param::Call),*>;
                type Sink = ($($param::Sink,)*);

                #[inline]
                fn key<T: Hasher>(&self, state: &mut T) {
                    $((self.0).$idx.key(state);)*
                }

                #[inline]
                fn call(&self, call: Self::Call) -> u128 {
                    match call {
                        $(MultiCall::$param($param) => (self.0).$idx.call($param)),*
                    }
                }

                #[inline]
                fn call_mut(&mut self, call: Self::Call) -> u128 {
                    match call {
                        $(MultiCall::$param($param) => (self.0).$idx.call_mut($param)),*
                    }
                }

                #[inline]
                fn retrack(
                    &mut self,
                    sink: impl Fn(Self::Call, u128) -> bool + Copy + Send + Sync + 'a,
                    bump: &'a Bump,
                ) {
                    $((self.0).$idx.retrack(
                        move |call, hash| sink(MultiCall::$param(call), hash),
                        bump,
                    );)*
                }
            }

            #[derive(PartialEq, Clone, Hash)]
            pub enum MultiCall<$($param),*> {
                $($param($param),)*
            }

            impl<$($param: Call),*> Call for MultiCall<$($param),*> {
                #[inline]
                fn is_mutable(&self) -> bool {
                    match *self {
                        $(Self::$param(ref $param) => $param.is_mutable(),)*
                    }
                }
            }
        };
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
