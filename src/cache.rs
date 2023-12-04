use std::borrow::Cow;
use std::cell::Cell;
use std::collections::HashMap;
use std::hash::{Hash, BuildHasherDefault};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, RwLock};

use once_cell::sync::Lazy;
use rustc_hash::{FxHashMap, FxHasher};
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::input::Input;

type FxIndexMap<K, V> = indexmap::IndexMap<K, V, BuildHasherDefault<FxHasher>>;

static CACHES: RwLock<Vec<fn(usize)>> = RwLock::new(Vec::new());

static ACCELERATOR: Lazy<Mutex<FxHashMap<(usize, u128), u128>>> =
    Lazy::new(|| Mutex::new(FxHashMap::default()));

pub fn register_cache(fun: fn(usize)) {
    CACHES.write().unwrap().push(fun);
}

thread_local! {
    static LAST_WAS_HIT: Cell<bool> = const { Cell::new(false) };
}

/// The global ID counter for tracked values. Each tracked value gets a
/// unqiue ID based on which its validations are cached in the accelerator.
/// IDs may only be reused upon eviction of the accelerator.
static ID: AtomicUsize = AtomicUsize::new(0);

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    mut input: In,
    constraint: &'c In::Constraint,
    cache: &RwLock<Cache<In::Constraint, Out>>,
    func: F,
) -> Out
where
    In: Input + 'c,
    Out: Clone + 'static,
    F: FnOnce(In::Tracked<'c>) -> Out,
{
    // Compute the hash of the input's key part.
    let key = {
        let mut state = SipHasher13::new();
        input.key(&mut state);
        state.finish128().as_u128()
    };

    // Check if there is a cached output.
    let mut borrow = cache.write().unwrap();
    if let Some(constrained) = borrow.lookup::<In>(key, &input) {
        // Replay the mutations.
        input.replay(&constrained.constraint);

        // Add the cached constraints to the outer ones.
        input.retrack(constraint).1.join(&constrained.constraint);

        let value = constrained.output.clone();
        LAST_WAS_HIT.with(|cell| cell.set(true));
        return value;
    }

    // Release the borrow so that nested memoized calls can access the
    // cache without panicking.
    drop(borrow);

    // Execute the function with the new constraints hooked in.
    let (input, outer) = input.retrack(constraint);
    let output = func(input);

    // Add the new constraints to the outer ones.
    outer.join(constraint);

    // Insert the result into the cache.
    borrow = cache.write().unwrap();
    borrow.insert::<In>(key, constraint.take(), output.clone());
    LAST_WAS_HIT.with(|cell| cell.set(false));

    output
}

/// Whether the last call was a hit.
pub fn last_was_hit() -> bool {
    LAST_WAS_HIT.with(|cell| cell.get())
}

/// Get the next ID.
pub fn id() -> usize {
    ID.fetch_add(1, Ordering::SeqCst)
}

/// Evict the cache.
///
/// This removes all memoized results from the cache whose age is larger than or
/// equal to `max_age`. The age of a result grows by one during each eviction
/// and is reset to zero when the result produces a cache hit. Set `max_age` to
/// zero to completely clear the cache.
///
/// Comemo's cache is thread-local, meaning that this only evicts this thread's
/// cache.
pub fn evict(max_age: usize) {
    CACHES.read().unwrap().iter().for_each(|fun| fun(max_age));
    ACCELERATOR.lock().unwrap().clear();
}

/// The global cache.
pub struct Cache<C, Out> {
    /// Maps from hashes to memoized results.
    entries: HashMap<u128, Vec<CacheEntry<C, Out>>>,
}

impl<C, Out> Default for Cache<C, Out> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}

impl<C, Out: 'static> Cache<C, Out> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn evict(&mut self, max_age: usize) {
        self.entries.retain(|_, entries| {
            entries.retain_mut(|entry| {
                entry.age += 1;
                entry.age <= max_age
            });
            !entries.is_empty()
        });
    }

    /// Look for a matching entry in the cache.
    fn lookup<In>(
        &mut self,
        key: u128,
        input: &In,
    ) -> Option<&Constrained<In::Constraint, Out>>
    where
        In: Input<Constraint = C>,
    {
        self.entries
            .get_mut(&key)?
            .iter_mut()
            .rev()
            .find_map(|entry| entry.lookup::<In>(input))
    }

    /// Insert an entry into the cache.
    fn insert<In>(&mut self, key: u128, constraint: In::Constraint, output: Out)
    where
        In: Input<Constraint = C>,
    {
        self.entries
            .entry(key)
            .or_default()
            .push(CacheEntry::new::<In>(constraint, output));
    }
}

/// A memoized result.
struct CacheEntry<C, Out> {
    /// The memoized function's constrained output.
    ///
    /// This is of type `Constrained<In::Constraint, Out>`.
    constrained: Constrained<C, Out>,
    /// How many evictions have passed since the entry has been last used.
    age: usize,
}

/// A value with a constraint.
struct Constrained<C, T> {
    /// The constraint which must be fulfilled for the output to be used.
    constraint: C,
    /// The memoized function's output.
    output: T,
}

impl<C, Out: 'static> CacheEntry<C, Out> {
    /// Create a new entry.
    fn new<In>(constraint: In::Constraint, output: Out) -> Self
    where
        In: Input<Constraint = C>,
    {
        Self {
            constrained: Constrained { constraint, output },
            age: 0,
        }
    }

    /// Return the entry's output if it is valid for the given input.
    fn lookup<In>(&mut self, input: &In) -> Option<&Constrained<In::Constraint, Out>>
    where
        In: Input<Constraint = C>,
    {
        input.validate(&self.constrained.constraint).then(|| {
            self.age = 0;
            &self.constrained
        })
    }
}

/// Defines a constraint for a tracked type.
pub struct Constraint<T>(RwLock<Inner<T>>);

struct Inner<T> {
    calls: Vec<Call<T>>,
    immutable: FxHashMap<u128, usize>,
}

impl<T: Clone> Clone for Constraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().unwrap().clone()))
    }
}

impl<T: Clone> Clone for Inner<T> {
    fn clone(&self) -> Self {
        Self {
            calls: self.calls.clone(),
            immutable: self.immutable.clone(),
        }
    }
}

/// A call entry.
#[derive(Clone)]
struct Call<T> {
    args: T,
    ret: u128,
    both: u128,
    mutable: bool,
}

impl<T: Hash + PartialEq + Clone> Inner<T> {
    /// Enter a constraint for a call to an immutable function.
    #[inline]
    fn push_inner(&mut self, call: Cow<Call<T>>) {
        if !call.mutable {
            if let Some(_prev) = self.immutable.get(&call.both) {
                #[cfg(debug_assertions)]
                {
                    let prev = &self.calls[*_prev];
                    if prev.args == call.args {
                        check(prev.ret, call.ret);
                    }
                }

                return;
            }
        }

        if call.mutable {
            self.immutable.clear();
        } else {
            self.immutable.insert(call.both, self.calls.len());
        }
        self.calls.push(call.into_owned());
    }
}

impl<T: Hash + PartialEq + Clone> Constraint<T> {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, args: T, ret: u128, mutable: bool) {
        let both = hash(&(&args, ret));
        self.0.write().unwrap().push_inner(Cow::Owned(Call { args, ret, both, mutable }));
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0
            .read()
            .unwrap()
            .calls
            .iter()
            .all(|entry| f(&entry.args) == entry.ret)
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, id: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        let inner = self.0.read().unwrap();
        let mut map = ACCELERATOR.lock().unwrap();
        inner.calls.iter().all(|entry| {
            *map.entry((id, entry.both)).or_insert_with(|| f(&entry.args)) == entry.ret
        })
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        for entry in self.0.read().unwrap().calls.iter() {
            if entry.mutable {
                f(&entry.args);
            }
        }
    }
}

impl<T> Default for Constraint<T> {
    fn default() -> Self {
        Self(RwLock::new(Inner { calls: Vec::new(), immutable: FxHashMap::default() }))
    }
}

impl<T> Default for Inner<T> {
    fn default() -> Self {
        Self { calls: Vec::new(), immutable: FxHashMap::default() }
    }
}

/// Defines a constraint for a tracked type.
pub struct ImmutableConstraint<T>(RwLock<FxIndexMap<u128, Call<T>>>);

impl<T: Clone> Clone for ImmutableConstraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().unwrap().clone()))
    }
}

impl<T: Hash + PartialEq + Clone> ImmutableConstraint<T> {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, args: T, ret: u128, mutable: bool) {
        let both = hash(&(&args, ret));
        self.push_inner(Cow::Owned(Call { args, ret, both, mutable }));
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    fn push_inner(&self, call: Cow<Call<T>>) {
        let mut calls = self.0.write().unwrap();
        debug_assert!(!call.mutable);

        if let Some(_prev) = calls.get(&call.both) {
            #[cfg(debug_assertions)]
            if _prev.args == call.args {
                check(_prev.ret, call.ret);
            }

            return;
        }

        calls.insert(call.both, call.into_owned());
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0
            .read()
            .unwrap()
            .values()
            .all(|entry| f(&entry.args) == entry.ret)
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, id: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        let calls = self.0.read().unwrap();
        let mut map = ACCELERATOR.lock().unwrap();
        calls.values().all(|entry| {
            *map.entry((id, entry.both)).or_insert_with(|| f(&entry.args)) == entry.ret
        })
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, _: F)
    where
        F: FnMut(&T),
    {
        for entry in self.0.read().unwrap().values() {
            debug_assert!(!entry.mutable);
        }
    }
}

impl<T> Default for ImmutableConstraint<T> {
    fn default() -> Self {
        Self(RwLock::new(FxIndexMap::with_hasher(Default::default())))
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
        let mut this = self.0.write().unwrap();
        for call in inner.0.read().unwrap().calls.iter() {
            this.push_inner(Cow::Borrowed(call));
        }
    }

    #[inline]
    fn take(&self) -> Self {
        Self(RwLock::new(std::mem::take(&mut *self.0.write().unwrap())))
    }
}

impl<T: Hash + Clone + PartialEq> Join for ImmutableConstraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        for call in inner.0.read().unwrap().values() {
            self.push_inner(Cow::Borrowed(call));
        }
    }

    #[inline]
    fn take(&self) -> Self {
        Self(RwLock::new(std::mem::take(&mut *self.0.write().unwrap())))
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
