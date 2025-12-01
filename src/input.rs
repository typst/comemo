use std::hash::{Hash, Hasher};

use crate::track::{Call, Sink, Track, Tracked, TrackedMut};

/// Ensure a type is suitable as input.
#[inline]
pub fn assert_hashable_or_trackable<'a, In: Input<'a>>(_: &In) {}

/// An input to a cached function.
///
/// This is implemented for hashable types, `Tracked<_>` types and `Args<(...)>`
/// types containing tuples up to length twelve.
pub trait Input<'a> {
    /// An enumeration of possible tracked calls that can be performed on any
    /// tracked part of this input.
    type Call: Call;

    /// Storage for a sink that combines the input's existing sink(s)
    /// (if any) with `S`.
    type Storage<S: Sink<Call = Self::Call> + 'a>: Default;

    /// Hashes the key (i.e. not tracked) parts of the input.
    fn key<H: Hasher>(&self, state: &mut H);

    /// Performs a call on a tracked part of the input and returns the hash of
    /// the result.
    ///
    /// If the call is mutable, the side effect will not be observable.
    fn call(&self, call: &Self::Call) -> u128;

    /// Performs a mutable call on a tracked part of the input.
    /// Mutable calls cannot have a return value and are only executed for their
    /// side effect. As such, this function does not return a result hash.
    fn call_mut(&mut self, call: &Self::Call);

    /// Integrates the given sink into the tracked parts of the input, using
    /// the external storage to store the new combined sink with lifetime 'a.
    fn attach<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a;
}

impl<'a, T: Hash> Input<'a> for T {
    // No sink for hashed inputs.
    type Call = ();
    type Storage<S: Sink<Call = Self::Call> + 'a> = ();

    #[inline]
    fn key<H: Hasher>(&self, state: &mut H) {
        Hash::hash(self, state);
    }

    #[inline]
    fn call(&self, _: &Self::Call) -> u128 {
        // No calls on hashed inputs.
        0
    }

    #[inline]
    fn call_mut(&mut self, _: &Self::Call) {}

    #[inline]
    fn attach<S>(&mut self, _: &'a mut Self::Storage<S>, _: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a,
    {
    }
}

impl<'a, T> Input<'a> for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    type Call = T::Call;
    type Storage<S: Sink<Call = Self::Call> + 'a> = Option<MergedSink<'a, S>>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: &Self::Call) -> u128 {
        let hash = if let Some(accelerator) = crate::accelerate::get(self.id) {
            // When we have an accelerator for this tracked instance, we might
            // already have a cached value of the return hash. Then, we don't
            // need to actually perform the call.
            let mut map = accelerator.lock();
            let call_hash = crate::hash::hash(call);
            *map.entry(call_hash).or_insert_with(|| self.value.call(call))
        } else {
            self.value.call(call)
        };

        // The `call` method is used during the constraint validation tree
        // traversal. It's crucial that we also send calls to the outer sink
        // here so that the outer sink observes the calls when we have a cache
        // hit. We do _not_ replay the constraints in another way.
        if let Some(sink) = self.sink {
            sink.emit(call.clone(), hash);
        }

        hash
    }

    #[inline]
    fn call_mut(&mut self, _: &Self::Call) {
        // Cannot perform a mutable call on an immutable reference.
    }

    #[inline]
    fn attach<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a,
    {
        self.sink = Some(storage.insert(MergedSink { prev: self.sink, sink }));
    }
}

impl<'a, T> Input<'a> for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    type Call = T::Call;
    type Storage<S: Sink<Call = Self::Call> + 'a> = Option<MergedSink<'a, S>>;

    #[inline]
    fn key<H: Hasher>(&self, _: &mut H) {}

    #[inline]
    fn call(&self, call: &Self::Call) -> u128 {
        let hash = self.value.call(call);
        if let Some(sink) = self.sink {
            sink.emit(call.clone(), hash);
        }
        hash
    }

    #[inline]
    fn call_mut(&mut self, call: &Self::Call) {
        self.value.call_mut(call);
        if let Some(sink) = self.sink {
            sink.emit(call.clone(), 0);
        }
    }

    #[inline]
    fn attach<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a,
    {
        self.sink = Some(storage.insert(MergedSink { prev: self.sink, sink }));
    }
}

/// Combines an existing sink from a tracked type with `S`.
#[derive(Copy, Clone)]
pub struct MergedSink<'a, S: Sink> {
    prev: Option<&'a dyn Sink<Call = S::Call>>,
    sink: S,
}

impl<'a, C, S> Sink for MergedSink<'a, S>
where
    C: Call,
    S: Sink<Call = C>,
{
    type Call = C;

    fn emit(&self, call: C, ret: u128) -> bool {
        if let Some(prev) = self.prev {
            // If the current sink already deduplicated the value, we don't have
            // to go the previous sink in the first place.
            self.sink.emit(call.clone(), ret) && prev.emit(call, ret)
        } else {
            self.sink.emit(call, ret)
        }
    }
}

/// Wrapper for multiple inputs.
pub struct Multi<T>(pub T);

macro_rules! multi {
    (@inner $($param:ident $alt:ident $idx:tt),*; $params:tt) => {
        impl<'a, $($param: Input<'a>),*> Input<'a> for Multi<($($param,)*)> {
            type Call = MultiCall<$($param::Call),*>;
            type Storage<S: Sink<Call = Self::Call> + 'a> =
                ($($param::Storage<MappedSink<$idx, S>>,)*);

            #[inline]
            fn key<T: Hasher>(&self, state: &mut T) {
                $((self.0).$idx.key(state);)*
            }

            #[inline]
            fn call(&self, call: &Self::Call) -> u128 {
                match *call {
                    $(MultiCall::$param(ref $param) => (self.0).$idx.call($param)),*
                }
            }

            #[inline]
            fn call_mut(&mut self, call: &Self::Call) {
                match *call {
                    $(MultiCall::$param(ref $param) => (self.0).$idx.call_mut($param)),*
                }
            }

            #[inline]
            fn attach<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
            where
                S: Sink<Call = Self::Call> + Copy + 'a {
                $((self.0).$idx.attach(&mut storage.$idx, MappedSink::<$idx, _>(sink));)*
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

        #[derive(Copy, Clone)]
        #[allow(dead_code)]
        pub struct MappedSink<const I: usize, S>(S);

        $(multi!(@mapped $param $idx; $params);)*
    };

    (@mapped $pick:ident $idx:tt; ($($param:ident),*)) => {
        impl<S, $($param),*> Sink for MappedSink<$idx, S>
        where
            S: Sink<Call = MultiCall<$($param),*>>,
        {
            type Call = $pick;

            fn emit(&self, call: $pick, ret: u128) -> bool {
                self.0.emit(MultiCall::$pick(call), ret)
            }
        }
    };

    ($($param:ident $alt:ident $idx:tt),*) => {
        #[allow(unused_variables, clippy::unused_unit, non_snake_case)]
        const _: () = {
            multi!(@inner $($param $alt $idx),*; ($($param),*));
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
