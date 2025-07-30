use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::RwLock;
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::accelerate;
use crate::constraint::Constraint;
use crate::input::Input;
use crate::track::Call;
use crate::tree::{CallTree, InsertError};

/// The global list of eviction functions.
static EVICTORS: RwLock<Vec<fn(usize)>> = RwLock::new(Vec::new());

/// Executes a function, trying to use a cached result for it.
#[allow(clippy::type_complexity)]
pub fn memoize<'a, In, Out, F>(
    cache: &Cache<In::Call, Out>,
    mut input: In,
    // These values must come from outside so that they have a lifetime that
    // allows them to be attached to the `input`. On the call site, they are
    // simply initialized as `&mut Default::default()`.
    (storage, constraint): &'a mut (
        In::Storage<&'a Constraint<In::Call>>,
        Constraint<In::Call>,
    ),
    enabled: bool,
    func: F,
) -> Out
where
    In: Input<'a>,
    Out: Clone + 'static,
    F: FnOnce(In) -> Out,
{
    // Early bypass if memoization is disabled.
    if !enabled {
        let output = func(input);

        // Ensure that the last call was a miss during testing.
        #[cfg(feature = "testing")]
        crate::testing::register_miss();

        return output;
    }

    // Compute the hash of the input's key part.
    let key = {
        let mut state = SipHasher13::new();
        input.key(&mut state);
        state.finish128().as_u128()
    };

    // Check if there is a cached output.
    if let Some(entry) = cache.0.read().lookup(key, &input) {
        // Replay mutations.
        for call in &entry.mutable {
            input.call_mut(call);
        }

        #[cfg(feature = "testing")]
        crate::testing::register_hit();

        return entry.output.clone();
    }

    // Attach the constraint.
    input.attach(storage, constraint);

    // Execute the function with the constraint attached.
    let output = func(input);

    // Insert the result into the cache.
    match cache.0.write().insert(key, constraint, output.clone()) {
        Ok(()) => {}
        Err(InsertError::AlreadyExists) => {
            // A concurrent call with the same arguments may have inserted
            // a value in the meantime. That's okay.
        }
        Err(InsertError::MissingCall) => {
            // A missing call indicates a bug from a comemo user. See the
            // documentation for `InsertError::MissingCall` for more details.
            #[cfg(debug_assertions)]
            panic!("comemo: memoized function is non-deterministic");
        }
    }

    #[cfg(feature = "testing")]
    crate::testing::register_miss();

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

/// A cache for a single memoized function.
pub struct Cache<C, Out>(LazyLock<RwLock<CacheData<C, Out>>>);

impl<C: 'static, Out: 'static> Cache<C, Out> {
    /// Create an empty cache.
    ///
    /// It must take an initialization function because the `evict` fn
    /// pointer cannot be passed as an argument otherwise the function
    /// passed to `Lazy::new` is a closure and not a function pointer.
    pub const fn new(init: fn() -> RwLock<CacheData<C, Out>>) -> Self {
        Self(LazyLock::new(init))
    }

    /// Evict all entries whose age is larger than or equal to `max_age`.
    pub fn evict(&self, max_age: usize) {
        self.0.write().evict(max_age);
    }
}

/// The internal data for a cache.
pub struct CacheData<C, Out> {
    /// Maps from hashes to memoized results.
    tree: CallTree<C, CacheEntry<C, Out>>,
}

impl<C, Out: 'static> CacheData<C, Out> {
    /// Evict all entries whose age is larger than or equal to `max_age`.
    fn evict(&mut self, max_age: usize) {
        self.tree.retain(|entry| {
            let age = entry.age.get_mut();
            *age += 1;
            *age <= max_age
        });
    }

    /// Look for a matching entry in the cache.
    fn lookup<'a, In>(&self, key: u128, input: &In) -> Option<&CacheEntry<C, Out>>
    where
        C: Call,
        In: Input<'a, Call = C>,
    {
        self.tree
            .get(key, |c| input.call(c))
            .inspect(|entry| entry.age.store(0, Ordering::SeqCst))
    }

    /// Insert an entry into the cache.
    fn insert(
        &mut self,
        key: u128,
        constraint: &Constraint<C>,
        output: Out,
    ) -> Result<(), InsertError>
    where
        C: Call,
    {
        let (immutable, mutable) = constraint.take();
        self.tree.insert(
            key,
            immutable,
            CacheEntry { output, mutable, age: AtomicUsize::new(0) },
        )
    }
}

/// A memoized result.
struct CacheEntry<C, Out> {
    /// The memoized function's output.
    output: Out,
    /// Mutable tracked calls that must be replayed.
    mutable: Vec<C>,
    /// How many evictions have passed since the entry has last been used.
    age: AtomicUsize,
}

impl<C, Out> Default for CacheData<C, Out> {
    fn default() -> Self {
        Self { tree: CallTree::new() }
    }
}
