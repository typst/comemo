use std::cell::RefCell;
use std::hash::Hash;

use siphasher::sip128::{Hasher128, SipHasher};

/// Defines a constraint for a tracked method without arguments.
pub struct Constraint<Call> {
    calls: RefCell<Vec<Entry<Call>>>,
}

/// A call entry.
struct Entry<Call> {
    call: Call,
    hash: u128,
    mutable: bool,
}

impl<Call> Constraint<Call>
where
    Call: Clone + PartialEq,
{
    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, call: Call, hash: u128, mutable: bool) {
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
    #[inline]
    pub fn valid<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&Call) -> u128,
    {
        self.calls.borrow().iter().all(|entry| f(&entry.call) == entry.hash)
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&Call),
    {
        for entry in self.calls.borrow().iter() {
            if entry.mutable {
                f(&entry.call);
            }
        }
    }
}

impl<Call> Default for Constraint<Call>
where
    Call: Clone + PartialEq,
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

impl<Call> Join for Constraint<Call>
where
    Call: Clone + PartialEq,
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
