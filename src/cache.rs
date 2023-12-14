use std::borrow::Cow;
use std::collections::HashMap;
use std::hash::Hash;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;

use once_cell::sync::Lazy;
use parking_lot::{
    MappedRwLockReadGuard, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard,
};
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::input::Input;

pub type Accelerator = Mutex<HashMap<u128, u128>>;

/// The global list of caches.
static CACHES: RwLock<Vec<fn(usize)>> = RwLock::new(Vec::new());

/// The global list of currently alive accelerators.
static ACCELERATORS: RwLock<(usize, Vec<Accelerator>)> = RwLock::new((0, Vec::new()));

/// The current ID of the accelerator.
static ID: AtomicUsize = AtomicUsize::new(0);

/// Register a cache in the global list.
pub fn register_cache(fun: fn(usize)) {
    CACHES.write().push(fun);
}

/// Generate a new accelerator.
/// Will allocate a new accelerator if the ID is larger than the current capacity.
pub fn id() -> usize {
    // Get the next ID.
    ID.fetch_add(1, Ordering::SeqCst)
}

/// Get an accelerator by ID.
fn accelerator(id: usize) -> Option<MappedRwLockReadGuard<'static, Accelerator>> {
    #[cold]
    fn resize_accelerators(len: usize) {
        let mut accelerators = ACCELERATORS.write();

        if len <= accelerators.1.len() {
            return;
        }

        accelerators.1.resize_with(len, || Mutex::new(HashMap::new()));
    }

    // We always lock the accelerators, as we need to make sure that the
    // accelerator is not removed while we are reading it.
    let mut accelerators = ACCELERATORS.read();

    let offset = accelerators.0;
    if id < offset {
        return None;
    }

    if id - offset >= accelerators.1.len() {
        drop(accelerators);
        resize_accelerators(id - offset + 1);
        accelerators = ACCELERATORS.read();
    }

    // Because we release the lock before resizing the accelerator,
    // we need to check again whether the ID is still valid because
    // another thread might evicted the cache.
    let i = id - accelerators.0;
    if id < offset {
        return None;
    }

    Some(RwLockReadGuard::map(accelerators, move |accelerators| &accelerators.1[i]))
}

#[cfg(feature = "last_was_hit")]
thread_local! {
    /// Whether the last call was a hit.
    static LAST_WAS_HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    mut input: In,
    constraint: &'c In::Constraint,
    cache: &Cache<In::Constraint, Out>,
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
    let borrow = cache.read();
    if let Some((constrained, value)) = borrow.lookup::<In>(key, &input) {
        // Replay the mutations.
        input.replay(constrained);

        // Add the cached constraints to the outer ones.
        input.retrack(constraint).1.join(constrained);

        #[cfg(feature = "last_was_hit")]
        LAST_WAS_HIT.with(|cell| cell.set(true));

        return value.clone();
    }

    // Release the borrow so that nested memoized calls can access the
    // cache without dead locking.
    drop(borrow);

    // Execute the function with the new constraints hooked in.
    let (input, outer) = input.retrack(constraint);
    let output = func(input);

    // Add the new constraints to the outer ones.
    outer.join(constraint);

    // Insert the result into the cache.
    let mut borrow = cache.write();
    borrow.insert::<In>(key, constraint.take(), output.clone());

    #[cfg(feature = "last_was_hit")]
    LAST_WAS_HIT.with(|cell| cell.set(false));

    output
}

/// Whether the last call was a hit.
#[cfg(feature = "last_was_hit")]
pub fn last_was_hit() -> bool {
    LAST_WAS_HIT.with(|cell| cell.get())
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
    for subevict in CACHES.read().iter() {
        subevict(max_age);
    }

    // Evict all accelerators.
    let mut accelerators = ACCELERATORS.write();

    // Update the offset.
    accelerators.0 = ID.load(Ordering::SeqCst);

    // Clear all accelerators while keeping the memory allocated.
    accelerators.1.iter_mut().for_each(|accelerator| {
        accelerator.lock().clear();
    })
}

pub struct Cache<C, Out>(Lazy<RwLock<CacheData<C, Out>>>);

impl<C: 'static, Out: 'static> Cache<C, Out> {
    /// Create an empty cache.
    ///
    /// It must take an initialization function because the `evict` fn
    /// pointer cannot be passed as an argument otherwise the function
    /// passed to `Lazy::new` is a closure and not a function pointer.
    pub const fn new(init: fn() -> RwLock<CacheData<C, Out>>) -> Self {
        Self(Lazy::new(init))
    }

    /// Write to the inner cache.
    pub fn write(&self) -> RwLockWriteGuard<'_, CacheData<C, Out>> {
        self.0.write()
    }

    /// Read from the inner cache.
    fn read(&self) -> RwLockReadGuard<'_, CacheData<C, Out>> {
        self.0.read()
    }
}

/// The global cache.
pub struct CacheData<C, Out> {
    /// Maps from hashes to memoized results.
    entries: HashMap<u128, Vec<CacheEntry<C, Out>>>,
}

impl<C, Out> Default for CacheData<C, Out> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
    }
}

impl<C, Out: 'static> CacheData<C, Out> {
    /// Create an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Evict all entries whose age is larger than or equal to `max_age`.
    pub fn evict(&mut self, max_age: usize) {
        self.entries.retain(|_, entries| {
            entries.retain_mut(|entry| {
                let age = entry.age.get_mut();
                *age += 1;
                *age <= max_age
            });
            !entries.is_empty()
        });
    }

    /// Look for a matching entry in the cache.
    fn lookup<In>(&self, key: u128, input: &In) -> Option<(&In::Constraint, &Out)>
    where
        In: Input<Constraint = C>,
    {
        self.entries
            .get(&key)?
            .iter()
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
    /// The memoized function's constraint.
    constraint: C,
    /// The memoized function's output.
    output: Out,
    /// How many evictions have passed since the entry has been last used.
    age: AtomicUsize,
}

impl<C, Out: 'static> CacheEntry<C, Out> {
    /// Create a new entry.
    fn new<In>(constraint: In::Constraint, output: Out) -> Self
    where
        In: Input<Constraint = C>,
    {
        Self { constraint, output, age: AtomicUsize::new(0) }
    }

    /// Return the entry's output if it is valid for the given input.
    fn lookup<In>(&self, input: &In) -> Option<(&In::Constraint, &Out)>
    where
        In: Input<Constraint = C>,
    {
        input.validate(&self.constraint).then(|| {
            self.age.store(0, Ordering::SeqCst);
            (&self.constraint, &self.output)
        })
    }
}

/// A call to a tracked function.
pub trait Call {
    /// Whether the call is mutable.
    fn is_mutable(&self) -> bool;
}

/// A call entry.
#[derive(Clone)]
struct ConstraintEntry<T: Call> {
    args: T,
    args_hash: u128,
    ret: u128,
}

/// Defines a constraint for a tracked type.
pub struct ImmutableConstraint<T: Call>(RwLock<HashMap<u128, ConstraintEntry<T>>>);

impl<T: Hash + PartialEq + Clone + Call> ImmutableConstraint<T> {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, args: T, ret: u128) {
        let args_hash = hash(&args);
        self.push_inner(Cow::Owned(ConstraintEntry { args, args_hash, ret }));
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    fn push_inner(&self, call: Cow<ConstraintEntry<T>>) {
        let mut calls = self.0.write();
        debug_assert!(!call.args.is_mutable());

        if let Some(_prev) = calls.get(&call.args_hash) {
            #[cfg(debug_assertions)]
            check(_prev, &call);

            return;
        }

        calls.insert(call.args_hash, call.into_owned());
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.read().values().all(|entry| f(&entry.args) == entry.ret)
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, id: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        let accelerator = accelerator(id);
        let inner = self.0.read();
        if let Some(accelerator) = accelerator {
            let mut map = accelerator.lock();
            inner.values().all(|entry| {
                *map.entry(entry.args_hash).or_insert_with(|| f(&entry.args)) == entry.ret
            })
        } else {
            inner.values().all(|entry| f(&entry.args) == entry.ret)
        }
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, _: F)
    where
        F: FnMut(&T),
    {
        #[cfg(debug_assertions)]
        for entry in self.0.read().values() {
            assert!(!entry.args.is_mutable());
        }
    }
}

impl<T: Clone + Call> Clone for ImmutableConstraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().clone()))
    }
}

impl<T: Call> Default for ImmutableConstraint<T> {
    fn default() -> Self {
        Self(RwLock::new(HashMap::default()))
    }
}

/// Defines a constraint for a tracked type.
pub struct MutableConstraint<T: Call>(RwLock<Inner<T>>);

impl<T: Hash + PartialEq + Clone + Call> MutableConstraint<T> {
    /// Create empty constraints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Enter a constraint for a call to an immutable function.
    #[inline]
    pub fn push(&self, args: T, ret: u128) {
        let args_hash = hash(&args);
        self.0
            .write()
            .push_inner(Cow::Owned(ConstraintEntry { args, args_hash, ret }));
    }

    /// Whether the method satisfies as all input-output pairs.
    #[inline]
    pub fn validate<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        self.0.read().calls.iter().all(|entry| f(&entry.args) == entry.ret)
    }

    /// Whether the method satisfies as all input-output pairs.
    ///
    /// On mutable tracked types, this does not use an accelerator as it is
    /// rarely, if ever used. Therefore, it is not worth the overhead.
    #[inline]
    pub fn validate_with_id<F>(&self, mut f: F, _: usize) -> bool
    where
        F: FnMut(&T) -> u128,
    {
        let inner = self.0.read();
        inner.calls.iter().all(|entry| f(&entry.args) == entry.ret)
    }

    /// Replay all input-output pairs.
    #[inline]
    pub fn replay<F>(&self, mut f: F)
    where
        F: FnMut(&T),
    {
        for call in self.0.read().calls.iter().filter(|call| call.args.is_mutable()) {
            f(&call.args);
        }
    }
}

impl<T: Clone + Call> Clone for MutableConstraint<T> {
    fn clone(&self) -> Self {
        Self(RwLock::new(self.0.read().clone()))
    }
}

impl<T: Call> Default for MutableConstraint<T> {
    fn default() -> Self {
        Self(RwLock::new(Inner { calls: Vec::new() }))
    }
}

#[derive(Clone)]
struct Inner<T: Call> {
    /// The list of calls.
    ///
    /// Order matters here, as those are mutable & immutable calls.
    calls: Vec<ConstraintEntry<T>>,
}

impl<T: Hash + PartialEq + Clone + Call> Inner<T> {
    /// Enter a constraint for a call to a function.
    ///
    /// If the function is immutable, it uses a fast-path based on a
    /// `HashMap` to perform deduplication. Otherwise, it always
    /// pushes the call to the list.
    #[inline]
    fn push_inner(&mut self, call: Cow<ConstraintEntry<T>>) {
        // If the call is immutable check whether we already have a call
        // with the same arguments and return value.
        let mutable = call.args.is_mutable();
        if !mutable {
            for entry in self.calls.iter().rev() {
                if call.args.is_mutable() {
                    break;
                }

                if call.args_hash == entry.args_hash && call.ret == entry.ret {
                    #[cfg(debug_assertions)]
                    check(&call, entry);

                    return;
                }
            }
        }

        // Insert the call into the call list.
        self.calls.push(call.into_owned());
    }
}

impl<T: Call> Default for Inner<T> {
    fn default() -> Self {
        Self { calls: Vec::new() }
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

impl<T: Hash + Clone + PartialEq + Call> Join for MutableConstraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        let mut this = self.0.write();
        for call in inner.0.read().calls.iter() {
            this.push_inner(Cow::Borrowed(call));
        }
    }

    #[inline]
    fn take(&self) -> Self {
        Self(RwLock::new(std::mem::take(&mut *self.0.write())))
    }
}

impl<T: Hash + Clone + PartialEq + Call> Join for ImmutableConstraint<T> {
    #[inline]
    fn join(&self, inner: &Self) {
        for call in inner.0.read().values() {
            self.push_inner(Cow::Borrowed(call));
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
fn check<T: PartialEq + Call>(lhs: &ConstraintEntry<T>, rhs: &ConstraintEntry<T>) {
    if lhs.ret != rhs.ret {
        panic!(
            "comemo: found conflicting constraints. \
             is this tracked function pure?"
        )
    }

    // Additional checks for debugging.
    if lhs.args_hash != rhs.args_hash || lhs.args != rhs.args {
        panic!(
            "comemo: found conflicting `check` arguments. \
             this is a bug in comemo"
        )
    }
}
