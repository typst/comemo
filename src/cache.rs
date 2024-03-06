use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use once_cell::sync::Lazy;
use parking_lot::RwLock;
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::accelerate;
use crate::constraint::Join;
use crate::input::Input;

/// The global list of eviction functions.
static EVICTORS: RwLock<Vec<fn(usize)>> = RwLock::new(Vec::new());

#[cfg(feature = "testing")]
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
    let borrow = cache.0.read();
    if let Some((constrained, value)) = borrow.lookup::<In>(key, &input) {
        // Replay the mutations.
        input.replay(constrained);

        // Add the cached constraints to the outer ones.
        input.retrack(constraint).1.join(constrained);

        #[cfg(feature = "testing")]
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
    let mut borrow = cache.0.write();
    borrow.insert::<In>(key, constraint.take(), output.clone());

    #[cfg(feature = "testing")]
    LAST_WAS_HIT.with(|cell| cell.set(false));

    output
}

/// Evict the global cache.
///
/// This removes all memoized results from the cache whose age is larger than or
/// equal to `max_age`. The age of a result grows by one during each eviction
/// and is reset to zero when the result produces a cache hit. Set `max_age` to
/// zero to completely clear the cache.
pub fn evict(max_age: usize) {
    for subevict in EVICTORS.read().iter() {
        subevict(max_age);
    }

    accelerate::evict();
}

/// Register an eviction function in the global list.
pub fn register_evictor(evict: fn(usize)) {
    EVICTORS.write().push(evict);
}

/// Whether the last call was a hit.
#[cfg(feature = "testing")]
pub fn last_was_hit() -> bool {
    LAST_WAS_HIT.with(|cell| cell.get())
}

/// A cache for a single memoized function.
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

    /// Evict all entries whose age is larger than or equal to `max_age`.
    pub fn evict(&self, max_age: usize) {
        self.0.write().evict(max_age)
    }
}

/// The internal data for a cache.
pub struct CacheData<C, Out> {
    /// Maps from hashes to memoized results.
    entries: HashMap<u128, Vec<CacheEntry<C, Out>>>,
}

impl<C, Out: 'static> CacheData<C, Out> {
    /// Evict all entries whose age is larger than or equal to `max_age`.
    fn evict(&mut self, max_age: usize) {
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

impl<C, Out> Default for CacheData<C, Out> {
    fn default() -> Self {
        Self { entries: HashMap::new() }
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
