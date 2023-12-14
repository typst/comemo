use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;

use parking_lot::RwLock;
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::accelerate;

/// A call to a tracked function.
pub trait Call: Hash + PartialEq + Clone {
    /// Whether the call is mutable.
    fn is_mutable(&self) -> bool;
}

/// A constraint entry for a single call.
#[derive(Clone)]
struct ConstraintEntry<T: Call> {
    call: T,
    call_hash: u128,
    ret_hash: u128,
}

/// Defines a constraint for an immutably tracked type.
pub struct ImmutableConstraint<T: Call>(RwLock<EntryMap<T>>);

impl<T: Call> ImmutableConstraint<T> {
    /// Create an empty constraint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, call: T, ret_hash: u128) {
        let call_hash = hash(&call);
        let entry = ConstraintEntry { call, call_hash, ret_hash };
        self.0.write().push_inner(Cow::Owned(entry));
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.read().0.values().all(|entry| f(&entry.call) == entry.ret_hash)
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, id: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        let guard = self.0.read();
        if let Some(accelerator) = accelerate::get(id) {
            let mut map = accelerator.lock();
            guard.0.values().all(|entry| {
                *map.entry(entry.call_hash).or_insert_with(|| f(&entry.call))
                    == entry.ret_hash
            })
        } else {
            guard.0.values().all(|entry| f(&entry.call) == entry.ret_hash)
        }
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, _: F)
    where
        F: FnMut(&T),
    {
        #[cfg(debug_assertions)]
        for entry in self.0.read().0.values() {
            assert!(!entry.call.is_mutable());
        }
    }
}

impl<T: Call> Clone for ImmutableConstraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().clone()))
    }
}

impl<T: Call> Default for ImmutableConstraint<T> {
    fn default() -> Self {
        Self(RwLock::new(EntryMap::default()))
    }
}

/// Defines a constraint for a mutably tracked type.
pub struct MutableConstraint<T: Call>(RwLock<EntryVec<T>>);

impl<T: Call> MutableConstraint<T> {
    /// Create an empty constraint.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to a mutable function.
    #[inline]
    pub fn push(&self, call: T, ret_hash: u128) {
        let call_hash = hash(&call);
        let entry = ConstraintEntry { call, call_hash, ret_hash };
        self.0.write().push_inner(Cow::Owned(entry));
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.read().0.iter().all(|entry| f(&entry.call) == entry.ret_hash)
    }

    /// Whether the method satisfies as all input-output pairs.
    ///
    /// On mutable tracked types, this does not use an accelerator as it is
    /// rarely, if ever used. Therefore, it is not worth the overhead.
    #[inline]
    pub fn validate_with_id<F>(&self, f: F, _: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.validate(f)
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        for entry in &self.0.read().0 {
            if entry.call.is_mutable() {
                f(&entry.call);
            }
        }
    }
}

impl<T: Call> Clone for MutableConstraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().clone()))
    }
}

impl<T: Call> Default for MutableConstraint<T> {
    fn default() -> Self {
        Self(RwLock::new(EntryVec::default()))
    }
}

/// A map of calls.
#[derive(Clone)]
struct EntryMap<T: Call>(HashMap<u128, ConstraintEntry<T>>);

impl<T: Call> EntryMap<T> {
    /// Enter a constraint for a call to a function.
    #[inline]
    fn push_inner(&mut self, entry: Cow<ConstraintEntry<T>>) {
        match self.0.entry(entry.call_hash) {
            Entry::Occupied(_occupied) => {
                #[cfg(debug_assertions)]
                check(_occupied.get(), &entry);
            }
            Entry::Vacant(vacant) => {
                vacant.insert(entry.into_owned());
            }
        }
    }
}

impl<T: Call> Default for EntryMap<T> {
    fn default() -> Self {
        Self(HashMap::new())
    }
}

/// A list of calls.
///
/// Order matters here, as those are mutable & immutable calls.
#[derive(Clone)]
struct EntryVec<T: Call>(Vec<ConstraintEntry<T>>);

impl<T: Call> EntryVec<T> {
    /// Enter a constraint for a call to a function.
    #[inline]
    fn push_inner(&mut self, entry: Cow<ConstraintEntry<T>>) {
        // If the call is immutable check whether we already have a call
        // with the same arguments and return value.
        if !entry.call.is_mutable() {
            for prev in self.0.iter().rev() {
                if entry.call.is_mutable() {
                    break;
                }

                if entry.call_hash == prev.call_hash && entry.ret_hash == prev.ret_hash {
                    #[cfg(debug_assertions)]
                    check(&entry, prev);
                    return;
                }
            }
        }

        // Insert the call into the call list.
        self.0.push(entry.into_owned());
    }
}

impl<T: Call> Default for EntryVec<T> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

/// Extend an outer constraint by an inner one.
pub trait Join<T = Self> {
    /// Join this constraint with the `inner` one.
    fn join(&self, inner: &T);

    /// Take out the constraint.
    fn take(&self) -> Self;
}

impl<T: Join> Join<T> for Option<&T> {
    #[inline]
    fn join(&self, inner: &T) {
        if let Some(outer) = self {
            outer.join(inner);
        }
    }

    #[inline]
    fn take(&self) -> Self {
        unimplemented!("cannot call `Join::take` on optional constraint")
    }
}

impl<T: Call> Join for ImmutableConstraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        let mut this = self.0.write();
        for entry in inner.0.read().0.values() {
            this.push_inner(Cow::Borrowed(entry));
        }
    }

    #[inline]
    fn take(&self) -> Self {
        Self(RwLock::new(std::mem::take(&mut *self.0.write())))
    }
}

impl<T: Call> Join for MutableConstraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        let mut this = self.0.write();
        for entry in inner.0.read().0.iter() {
            this.push_inner(Cow::Borrowed(entry));
        }
    }

    #[inline]
    fn take(&self) -> Self {
        Self(RwLock::new(std::mem::take(&mut *self.0.write())))
    }
}

/// Produce a 128-bit hash of a value.
#[inline]
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher13::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}

/// Check for a constraint violation.
#[inline]
#[track_caller]
#[allow(dead_code)]
fn check<T: Call>(lhs: &ConstraintEntry<T>, rhs: &ConstraintEntry<T>) {
    if lhs.ret_hash != rhs.ret_hash {
        panic!(
            "comemo: found conflicting constraints. \
             is this tracked function pure?"
        )
    }

    // Additional checks for debugging.
    if lhs.call_hash != rhs.call_hash || lhs.call != rhs.call {
        panic!(
            "comemo: found conflicting `check` arguments. \
             this is a bug in comemo"
        )
    }
}
