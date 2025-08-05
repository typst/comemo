use std::collections::hash_map::Entry;
use std::hash::Hash;

use parking_lot::Mutex;

use crate::Track;
use crate::passthroughhasher::PassthroughHashMap;
use crate::track::{Call, Sink};

/// Records calls performed on a trackable type.
///
/// Allows to validate that a different instance of the trackable type yields
/// the same outputs for the recorded calls.
///
/// The constraint can be hooked up to a tracked type through
/// [`Track::track_with`].
pub struct Constraint<C>(Mutex<ConstraintRepr<C>>);

/// The internal representation of a [`Constraint`].
struct ConstraintRepr<C> {
    /// The immutable calls, ready for integration into a call tree.
    immutable: CallSequence<C>,
    /// The mutable calls, for insertion as part of the call tree output value.
    mutable: Vec<C>,
}

impl<C> Constraint<C> {
    /// Creates a new constraint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Checks whether the given value matches the constraints by invoking the
    /// recorded calls one by one.
    pub fn validate<T>(&self, value: &T) -> bool
    where
        T: Track<Call = C>,
    {
        self.0
            .lock()
            .immutable
            .vec
            .iter()
            .filter_map(|x| x.as_ref())
            .all(|(call, ret)| value.call(call) == *ret)
    }

    /// Takes out the immutable and mutable calls, for insertion into a call
    /// tree.
    pub(crate) fn take(&self) -> (CallSequence<C>, Vec<C>) {
        let mut inner = self.0.lock();
        (std::mem::take(&mut inner.immutable), std::mem::take(&mut inner.mutable))
    }
}

impl<C> Default for Constraint<C> {
    fn default() -> Self {
        Self(Mutex::new(ConstraintRepr {
            immutable: CallSequence::new(),
            mutable: Vec::new(),
        }))
    }
}

impl<C: Call> Sink for Constraint<C> {
    type Call = C;

    fn emit(&self, call: C, ret: u128) -> bool {
        let mut inner = self.0.lock();
        if call.is_mutable() {
            inner.mutable.push(call);
            true
        } else {
            inner.immutable.insert(call, ret)
        }
    }
}

/// A deduplicated sequence of calls to tracked functions, optimized for
/// efficient insertion into a call tree.
pub struct CallSequence<C> {
    /// The raw calls. In order, but deduplicated via the `map`.
    vec: Vec<Option<(C, u128)>>,
    /// A map from hashes of calls to the indices in the vector.
    map: PassthroughHashMap<u128, usize>,
    /// A cursor for iteration in `Self::next`.
    cursor: usize,
}

impl<C> CallSequence<C> {
    /// Creates an empty sequence.
    pub fn new() -> Self {
        Self {
            vec: Vec::new(),
            map: PassthroughHashMap::default(),
            cursor: 0,
        }
    }
}

impl<C: Hash> CallSequence<C> {
    /// Inserts a pair of a call and its return hash.
    ///
    /// Returns true when the pair was indeed inserted and false if the call was
    /// deduplicated.
    pub fn insert(&mut self, call: C, ret: u128) -> bool {
        match self.map.entry(crate::hash::hash(&call)) {
            Entry::Vacant(entry) => {
                let i = self.vec.len();
                self.vec.push(Some((call, ret)));
                entry.insert(i);
                true
            }
            #[allow(unused_variables)]
            Entry::Occupied(entry) => {
                #[cfg(debug_assertions)]
                if let Some((_, ret2)) = &self.vec[*entry.get()] {
                    if ret != *ret2 {
                        panic!(
                            "comemo: found differing return values. \
                             is there an impure tracked function?"
                        )
                    }
                }
                false
            }
        }
    }

    /// Retrieves the next call in order.
    pub fn next(&mut self) -> Option<(C, u128)> {
        while self.cursor < self.vec.len() {
            if let Some(pair) = self.vec[self.cursor].take() {
                return Some(pair);
            }
            self.cursor += 1;
        }
        None
    }

    /// Retrieves the return hash of an arbitrary upcoming call. Removes the
    /// call from the sequence; it will not be yielded by `next()` anymore.
    pub fn extract(&mut self, call: &C) -> Option<u128> {
        let h = crate::hash::hash(&call);
        let i = *self.map.get(&h)?;
        let res = self.vec[i].take().map(|(_, ret)| ret);
        debug_assert!(self.cursor <= i || res.is_none());
        res
    }
}

impl<C> Default for CallSequence<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Hash> FromIterator<(C, u128)> for CallSequence<C> {
    fn from_iter<T: IntoIterator<Item = (C, u128)>>(iter: T) -> Self {
        let mut seq = CallSequence::new();
        for (call, ret) in iter {
            seq.insert(call, ret);
        }
        seq
    }
}
