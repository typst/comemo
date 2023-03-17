use std::cell::RefCell;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};

use crate::track::Trackable;

/// Defines a constraint for a tracked method without arguments.
pub struct Constraint<T>
where
    T: Trackable + ?Sized,
{
    calls: RefCell<Vec<Entry<T::Call>>>,
}

/// A call entry.
struct Entry<Call> {
    call: Call,
    hash: u128,
    mutable: bool,
}

impl<T> Constraint<T>
where
    T: Trackable + ?Sized,
{
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    ///
    /// This method is not part of the public API!
    #[inline]
    #[doc(hidden)]
    pub fn push(&self, call: T::Call, hash: u128, mutable: bool) {
        let mut calls = self.calls.borrow_mut();

        if !mutable {
            for prev in calls.iter().rev() {
                if prev.mutable {
                    break;
                }

                if prev.call == call {
                    check(prev.hash, hash);
                    return;
                }
            }
        }

        calls.push(Entry { call, hash, mutable });
    }

    /// Whether the method satisfies as all input-output pairs.
    ///
    /// This method is not part of the public API!
    #[inline]
    #[doc(hidden)]
    pub fn valid<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T::Call) -> u128,
    {
        self.calls.borrow().iter().all(|entry| f(&entry.call) == entry.hash)
    }

    /// Replay all input-output pairs.
    ///
    /// This method is not part of the public API!
    #[inline]
    #[doc(hidden)]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&T::Call),
    {
        for entry in self.calls.borrow().iter() {
            if entry.mutable {
                f(&entry.call);
            }
        }
    }
}

impl<T> Default for Constraint<T>
where
    T: Trackable + ?Sized,
{
    fn default() -> Self {
        Self { calls: RefCell::new(vec![]) }
    }
}

/// Extend an outer constraint by an inner one.
pub trait Join<T = Self> {
    /// Join this constraint with the `inner` one.
    fn join(&self, inner: &T);
}

impl<T: Join> Join<T> for Option<&T> {
    #[inline]
    fn join(&self, inner: &T) {
        if let Some(outer) = self {
            outer.join(inner);
        }
    }
}

impl<T> Join for Constraint<T>
where
    T: Trackable + ?Sized,
{
    #[inline]
    fn join(&self, inner: &Self) {
        for inner in inner.calls.borrow().iter() {
            self.push(inner.call.clone(), inner.hash, inner.mutable);
        }
    }
}

/// Produce a 128-bit hash of a value.
#[inline]
pub fn hash<T: Hash>(value: &T) -> u128 {
    let mut state = SipHasher::new();
    value.hash(&mut state);
    state.finish128().as_u128()
}

/// Check for a constraint violation.
#[inline]
#[track_caller]
fn check(left_hash: u128, right_hash: u128) {
    if left_hash != right_hash {
        panic!(
            "comemo: found conflicting constraints. \
             is this tracked function pure?"
        )
    }
}
