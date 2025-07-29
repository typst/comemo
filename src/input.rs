use std::hash::{Hash, Hasher};

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
    type Storage<S: Sink<Call = Self::Call> + 'a>: Default;

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
    fn retrack<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a;
}

impl<'a, T: Hash> Input<'a> for T {
    // No constraint for hashed inputs.
    type Call = ();
    type Storage<S: Sink<Call = Self::Call> + 'a> = ();

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
    fn retrack<S>(&mut self, _: &'a mut Self::Storage<S>, _: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a,
    {
    }
}

impl<'a, T> Input<'a> for Tracked<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Call = T::Call;
    type Storage<S: Sink<Call = Self::Call> + 'a> = Option<TrackedSink<'a, S>>;

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
    fn retrack<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
    where
        S: Sink<Call = Self::Call> + 'a,
    {
        self.sink = Some(storage.insert(TrackedSink { prev: self.sink, sink }));
    }
}

impl<'a, T> Input<'a> for TrackedMut<'a, T>
where
    T: Track + ?Sized,
{
    // Forward constraint from `Trackable` implementation.
    type Call = T::Call;
    type Storage<S: Sink<Call = Self::Call> + 'a> = Option<TrackedSink<'a, S>>;

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
    fn retrack<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
    where
        S: Sink<Call = Self::Call> + Copy + 'a,
    {
        self.sink = Some(storage.insert(TrackedSink { prev: self.sink, sink }));
    }
}

#[derive(Copy, Clone)]
pub struct TrackedSink<'a, S: Sink> {
    prev: Option<&'a dyn Sink<Call = S::Call>>,
    sink: S,
}

impl<'a, C, S> Sink for TrackedSink<'a, S>
where
    C: Call,
    S: Sink<Call = C>,
{
    type Call = C;

    fn emit(&self, call: C, ret: u128) -> bool {
        if self.sink.emit(call.clone(), ret)
            && let Some(prev) = self.prev
        {
            prev.emit(call, ret)
        } else {
            true
        }
    }
}

/// Wrapper for multiple inputs.
pub struct Multi<T>(pub T);

#[expect(unused)]
macro_rules! multi {
    ($($param:tt $alt:tt $idx:tt),*) => {
        #[allow(unused_variables, clippy::unused_unit, non_snake_case)]
        const _: () = {
            impl<'a, $($param: Input<'a>),*> Input<'a> for Multi<($($param,)*)> {
                type Call = MultiCall<$($param::Call),*>;

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
                    sink: impl Sink<Call = Self::Call> + Copy + 'a,
                    bump: &'a Bump,
                ) {
                    $((self.0).$idx.retrack(
                        Redirect::<_, $idx>(sink),
                        // move |call, hash| sink.emit(MultiCall::$param(call), hash),
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

            // struct Foo;

            // impl Sink for Foo {
            //     fn emit(&self, call: C, ret: u128) -> bool {
            //         self.0.emit(MultiCall::$param(call), ret)
            //     }
            // }
        };
    };
}

// multi! {}
// multi! { A Z 0 }
// multi! { A Z 0, B Y 1 }
// multi! { A Z 0, B Y 1, C X 2 }
// multi! { A Z 0, B Y 1, C X 2, D W 3 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9, K P 10 }
// multi! { A Z 0, B Y 1, C X 2, D W 3, E V 4, F U 5, G T 6, H S 7, I R 8, J Q 9, K P 10, L O 11 }

#[allow(unused_variables, clippy::unused_unit, non_snake_case)]
const _: () = {
    impl<'a, A: Input<'a>, B: Input<'a>> Input<'a> for Multi<(A, B)> {
        type Call = MultiCall<A::Call, B::Call>;
        type Storage<S: Sink<Call = Self::Call> + 'a> =
            (A::Storage<Redirect<0, S>>, B::Storage<Redirect<1, S>>);

        #[inline]
        fn key<T: Hasher>(&self, state: &mut T) {
            (self.0).0.key(state);
            (self.0).1.key(state);
        }
        #[inline]
        fn call(&self, call: Self::Call) -> u128 {
            match call {
                MultiCall::A(A) => (self.0).0.call(A),
                MultiCall::B(B) => (self.0).1.call(B),
            }
        }
        #[inline]
        fn call_mut(&mut self, call: Self::Call) -> u128 {
            match call {
                MultiCall::A(A) => (self.0).0.call_mut(A),
                MultiCall::B(B) => (self.0).1.call_mut(B),
            }
        }
        fn retrack<S>(&mut self, storage: &'a mut Self::Storage<S>, sink: S)
        where
            S: Sink<Call = Self::Call> + Copy + 'a,
        {
            (self.0).0.retrack(&mut storage.0, Redirect::<0, _>(sink));
            (self.0).1.retrack(&mut storage.1, Redirect::<1, _>(sink));
        }
    }
    #[derive(PartialEq, Clone, Hash)]
    pub enum MultiCall<A, B> {
        A(A),
        B(B),
    }
    impl<A: Call, B: Call> Call for MultiCall<A, B> {
        #[inline]
        fn is_mutable(&self) -> bool {
            match *self {
                Self::A(ref A) => A.is_mutable(),
                Self::B(ref B) => B.is_mutable(),
            }
        }
    }

    #[derive(Copy, Clone)]
    pub struct Redirect<const I: usize, S>(S);

    impl<S, A, B> Sink for Redirect<0, S>
    where
        S: Sink<Call = MultiCall<A, B>>,
    {
        type Call = A;

        fn emit(&self, call: A, ret: u128) -> bool {
            self.0.emit(MultiCall::A(call), ret)
        }
    }

    impl<S, A, B> Sink for Redirect<1, S>
    where
        S: Sink<Call = MultiCall<A, B>>,
    {
        type Call = B;

        fn emit(&self, call: B, ret: u128) -> bool {
            self.0.emit(MultiCall::B(call), ret)
        }
    }
};
