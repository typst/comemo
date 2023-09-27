use std::hash::Hash;

use parking_lot::{RwLock, RwLockUpgradableReadGuard};
use siphasher::sip128::{Hasher128, SipHasher13};

/// Defines a constraint for a tracked type.
pub struct Constraint<T>(RwLock<Vec<Call<T>>>);

/// A call entry.
#[derive(Clone)]
struct Call<T> {
    args: T,
    ret: u128,
    both: u128,
    mutable: bool,
}

impl<T: Hash + PartialEq> Constraint<T> {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, args: T, ret: u128, mutable: bool) {
        let both = hash(&(&args, ret));
        self.push_inner(Call { args, ret, both, mutable });
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    fn push_inner(&self, call: Call<T>) {
        let calls = self.0.upgradable_read();

        if !call.mutable {
            for prev in calls.iter().rev() {
                if prev.mutable {
                    break;
                }

                #[cfg(debug_assertions)]
                if prev.args == call.args {
                    check(prev.ret, call.ret);
                }

                if prev.both == call.both {
                    return;
                }
            }
        }

        RwLockUpgradableReadGuard::upgrade(calls).push(call);
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.read().iter().all(|entry| f(&entry.args) == entry.ret)
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, id: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.read().iter().all(|entry| {
            *crate::ACCELERATOR
                .entry((id, entry.both))
                .or_insert_with(|| f(&entry.args))
                == entry.ret
        })
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        for entry in self.0.read().iter() {
            if entry.mutable {
                f(&entry.args);
            }
        }
    }
}

impl<T> Default for Constraint<T> {
    fn default() -> Self {
        Self(RwLock::new(vec![]))
    }
}

impl<T: Clone> Clone for Constraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().clone()))
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

impl<T: Hash + Clone + PartialEq> Join for Constraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        for call in inner.0.read().iter() {
            self.push_inner(call.clone());
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
fn check(left_hash: u128, right_hash: u128) {
    if left_hash != right_hash {
        panic!(
            "comemo: found conflicting constraints. \
             is this tracked function pure?"
        )
    }
}
