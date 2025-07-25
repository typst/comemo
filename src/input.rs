use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

use bumpalo::Bump;

use crate::call::Call;
use crate::track::{Sink, Track, Tracked, TrackedMut};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<In: Input>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input {
    /// An enumeration of possible tracked calls that can be performed on any
    /// tracked part of this input.
    type Call: Call;

    /// This type but with a different lifetime in its `Tracked` parts.
    type WithLifetime<'r>
    where
        Self: 'r;

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
    fn retrack<'r>(
        self,
        sink: impl Sink<Self::Call> + Copy + 'r,
        b: &'r Bump,
    ) -> Self::WithLifetime<'r>
    where
        Self: 'r;

    /// Returns this input with a reduced lifetime without any real changes.
    /// This is useful in generic code.
    fn reborrow<'r>(self) -> Self::WithLifetime<'r>
    where
        Self: 'r;
}

impl<T: Hash> Input for T {
    // No constraint for hashed inputs.
    type Call = ();
    type WithLifetime<'r>
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
        _: impl Sink<Self::Call> + Copy + 'r,
        _: &'r Bump,
    ) -> Self::WithLifetime<'r>
    where
        Self: 'r,
    {
        self
    }

    #[inline]
    fn reborrow<'r>(self) -> Self::WithLifetime<'r>
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
    type WithLifetime<'r>
        = Tracked<'r, T>
    where
        Self: 'r;

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
    fn retrack<'r>(
        self,
        sink: impl Sink<Self::Call> + Copy + 'r,
        b: &'r Bump,
    ) -> Self::WithLifetime<'r>
    where
        Self: 'r,
    {
        Tracked {
            value: self.value,
            id: self.id,
            sink: Some(b.alloc(TrackedSink { sink, outer: self.sink })),
        }
    }

    #[inline]
    fn reborrow<'r>(self) -> Self::WithLifetime<'r>
    where
        Self: 'r,
    {
        self
    }
}

struct TrackedSink<'a, C, S: Sink<C>> {
    sink: S,
    outer: Option<&'a dyn Sink<C>>,
}

impl<C: Clone, S: Sink<C>> Sink<C> for TrackedSink<'_, C, S> {
    fn emit(&self, call: C, ret: u128) -> bool {
        if self.sink.emit(call.clone(), ret) {
            if let Some(outer) = self.outer {
                return outer.emit(call, ret);
            }
            return true;
        }
        false
    }
}

impl<'a, T> Input for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Call = T::Call;
    type WithLifetime<'r>
        = TrackedMut<'r, T>
    where
        Self: 'r;

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
    fn retrack<'r>(
        self,
        sink: impl Sink<Self::Call> + Copy + 'r,
        b: &'r Bump,
    ) -> Self::WithLifetime<'r>
    where
        Self: 'r,
    {
        TrackedMut {
            value: self.value,
            sink: Some(b.alloc(TrackedSink { sink, outer: self.sink })),
        }
    }

    #[inline]
    fn reborrow<'r>(self) -> Self::WithLifetime<'r>
    where
        Self: 'r,
    {
        self
    }
}

/// Wrapper for multiple inputs.
pub struct Multi<T>(pub T);

macro_rules! multi {
    (@inner $($param:tt $alt:tt $idx:tt),*) => {
        impl<$($param: Input),*> Input for Multi<($($param,)*)> {
            type Call = MultiCall<$($param::Call),*>;
            type WithLifetime<'r> = ($($param::WithLifetime<'r>,)*) where Self: 'r;

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
            fn retrack<'r>(
                self,
                sink: impl Sink<Self::Call> + Copy + 'r,
                bump: &'r Bump,
            ) -> Self::WithLifetime<'r>
            where
                Self: 'r,
            {
                $(let $param = (self.0).$idx.retrack(
                    sinks::$param::Pick(sink, PhantomData),
                    bump,
                );)*
                ($($param,)*)
            }

            #[inline]
            fn reborrow<'r>(self) -> Self::WithLifetime<'r>
            where
                Self: 'r,
            {
                $(let $param = (self.0).$idx.reborrow();)*
                ($($param,)*)
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

        mod sinks {
            use super::*;
            multi!(@picks $($param,)*; ($($param,)*));
        }
    };

    (@picks $($param1:ident,)*; $rest:tt) => {
        $(pub mod $param1 {
            use super::*;
            multi!(@pick $param1; $rest);
        })*
    };

    (@pick $pick:ident; ($($param:ident,)*)) => {
        pub struct Pick<$($param,)* S>(pub S, pub PhantomData<fn(($($param,)*))>);

        impl<$($param,)* S> Sink<$pick> for Pick<$($param,)* S>
        where
            S: Sink<MultiCall<$($param,)*>>,
        {
            fn emit(&self, call: $pick, ret: u128) -> bool {
                self.0.emit(super::MultiCall::$pick(call), ret)
            }
        }

        impl<$($param,)* S: Copy> Copy for Pick<$($param,)* S> {}

        impl<$($param,)* S: Clone> Clone for Pick<$($param,)* S> {
            fn clone(&self) -> Self {
                Self(self.0.clone(), PhantomData)
            }
        }
    };

    ($($tts:tt)*) => {
        #[allow(unused_variables, unused_imports, clippy::unused_unit, non_snake_case)]
        const _: () = {
            mod _inner {
                use super::*;
                multi!(@inner $($tts)*);
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
