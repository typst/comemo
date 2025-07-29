use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{Mutex, RwLock};
use siphasher::sip128::{Hasher128, SipHasher13};

use crate::accelerate;
use crate::call::Call;
use crate::calltree::{CallSequence, CallTree, InsertError};
use crate::input::Input;
use crate::track::Sink;

/// The global list of eviction functions.
static EVICTORS: RwLock<Vec<fn(usize)>> = RwLock::new(Vec::new());

#[cfg(feature = "testing")]
thread_local! {
    /// Whether the last call was a hit.
    static LAST_WAS_HIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub struct Recording<C> {
    immutable: CallSequence<C>,
    mutable: Vec<C>,
}

impl<C> Default for Recording<C> {
    fn default() -> Self {
        Self {
            immutable: CallSequence::new(),
            mutable: Vec::new(),
        }
    }
}

impl<C: Call> Sink for &Mutex<Recording<C>> {
    type Call = C;

    fn emit(&self, call: C, ret: u128) -> bool {
        if call.is_mutable() {
            self.lock().mutable.push(call);
            true
        } else {
            self.lock().immutable.insert(call, ret)
        }
    }
}

static CACHES: LazyLock<RwLock<HashMap<TypeId, Box<dyn Any + Send + Sync>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

fn get_cache<C: Send + Sync + 'static, Out: Send + Sync + 'static>(
    id: TypeId,
) -> &'static Cache<C, Out> {
    let ptr: *const (dyn Any + Send + Sync) = if let Some(boxed) = CACHES.read().get(&id)
    {
        &**boxed as _
    } else {
        let mut borrowed = CACHES.write();
        let boxed = borrowed.entry(id).or_insert_with(|| {
            Box::new(Cache::<C, Out>::new()) as Box<dyn Any + Send + Sync>
        });
        &**boxed as _
    };

    unsafe { &*ptr as &'static (dyn Any + Send + Sync) }
        .downcast_ref::<Cache<C, Out>>()
        .unwrap()
}

/// Execute a function or use a cached result for it.
pub fn memoized<'c, In, Out, F>(
    mut input: In,
    storage: &'c mut In::Storage<&'c Mutex<Recording<In::Call>>>,
    sink: &'c Mutex<Recording<In::Call>>,
    id: TypeId,
    enabled: bool,
    func: F,
) -> Out
where
    In: Input<'c>,
    In::Call: 'static,
    Out: Clone + Send + Sync + 'static,
    F: FnOnce(In) -> Out,
{
    // Early bypass if memoization is disabled.
    // Hopefully the compiler will optimize this away, if the condition is constant.
    if !enabled {
        // Execute the function with the new constraints hooked in.
        let output = func(input);

        // Ensure that the last call was a miss during testing.
        #[cfg(feature = "testing")]
        LAST_WAS_HIT.with(|cell| cell.set(false));

        return output;
    }

    let cache = get_cache::<In::Call, Out>(id);

    // Compute the hash of the input's key part.
    let key = {
        let mut state = SipHasher13::new();
        input.key(&mut state);
        state.finish128().as_u128()
    };

    // Check if there is a cached output.
    if let Some(entry) = cache.0.read().lookup::<In>(key, &input) {
        entry.age.store(0, Ordering::SeqCst);

        // Replay mutations.
        for call in &entry.mutable {
            input.call_mut(call.clone());
        }

        #[cfg(feature = "testing")]
        LAST_WAS_HIT.with(|cell| cell.set(true));

        return entry.output.clone();
    }

    // Execute the function with the new constraints hooked in.
    input.retrack(storage, sink);
    let output = func(input);
    let list = std::mem::take(&mut *sink.lock());

    // Insert the result into the cache.
    match cache.0.write().insert::<In>(key, list, output.clone()) {
        Ok(()) => {}
        Err(InsertError::AlreadyExists) => {
            // A concurrent call with the same arguments can have inserted
            // a result in the meantime.
        }
        Err(InsertError::MissingCall) => {
            #[cfg(debug_assertions)]
            panic!("comemo: cached function is non-deterministic");
        }
    }

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
pub struct Cache<C, Out>(RwLock<CacheInner<C, Out>>);

impl<C, Out: 'static> Cache<C, Out> {
    /// Create an empty cache.
    ///
    /// It must take an initialization function because the `evict` fn
    /// pointer cannot be passed as an argument otherwise the function
    /// passed to `Lazy::new` is a closure and not a function pointer.
    pub fn new() -> Self {
        Self(RwLock::new(Default::default()))
    }
}

impl<C, Out: 'static> Default for Cache<C, Out> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: Call, Out: 'static> Cache<C, Out> {
    /// Evict all entries whose age is larger than or equal to `max_age`.
    pub fn evict(&self, max_age: usize) {
        self.0.write().evict(max_age);
    }
}

/// The data for a cache.
pub struct CacheInner<C, Out> {
    /// Maps from hashes to memoized results.
    tree: CallTree<C, CacheEntry<C, Out>>,
}

/// A memoized result.
struct CacheEntry<C, Out> {
    /// The memoized function's output.
    output: Out,
    /// Mutable tracked calls that must be replayed.
    mutable: Vec<C>,
    /// How many evictions have passed since the entry has been last used.
    age: AtomicUsize,
}

impl<C: Call, Out: 'static> CacheInner<C, Out> {
    /// Look for a matching entry in the cache.
    fn lookup<'c, In>(&self, key: u128, input: &In) -> Option<&CacheEntry<C, Out>>
    where
        In: Input<'c, Call = C>,
    {
        self.tree.get(key, |c| input.call(c.clone()))
    }

    /// Insert an entry into the cache.
    #[allow(clippy::extra_unused_type_parameters, reason = "false positive")]
    fn insert<'c, In>(
        &mut self,
        key: u128,
        recording: Recording<C>,
        output: Out,
    ) -> Result<(), InsertError>
    where
        In: Input<'c, Call = C>,
    {
        self.tree.insert(
            key,
            recording.immutable,
            CacheEntry {
                output,
                mutable: recording.mutable,
                age: AtomicUsize::new(0),
            },
        )
    }

    /// Evict all entries whose age is larger than or equal to `max_age`.
    fn evict(&mut self, max_age: usize) {
        self.tree.retain(|entry| {
            let age = entry.age.get_mut();
            *age += 1;
            *age <= max_age
        });
    }
}

impl<C, Out> Default for CacheInner<C, Out> {
    fn default() -> Self {
        Self { tree: CallTree::new() }
    }
}
