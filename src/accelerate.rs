use std::sync::atomic::{AtomicUsize, Ordering};

use parking_lot::{MappedRwLockReadGuard, Mutex, RwLock, RwLockReadGuard};
use rustc_hash::FxHashMap;

/// The global list of currently alive accelerators.
static ACCELERATORS: RwLock<(usize, Vec<Accelerator>)> = RwLock::new((0, Vec::new()));

/// The current ID of the accelerator.
static ID: AtomicUsize = AtomicUsize::new(0);

/// The type of each individual accelerator.
///
/// Maps from call hashes to return hashes.
type Accelerator = Mutex<FxHashMap<u128, u128>>;

/// Generate a new accelerator.
pub fn id() -> usize {
    // Get the next ID.
    ID.fetch_add(1, Ordering::SeqCst)
}

/// Evict the accelerators.
pub fn evict(release: bool) {
    let mut accelerators = ACCELERATORS.write();
    let (offset, vec) = &mut *accelerators;

    *offset = ID.load(Ordering::SeqCst);
    evict_pool(vec, release);
}

/// Retains used buffers for reuse, but releases idle or explicitly cleared storage.
fn evict_pool(vec: &mut Vec<Accelerator>, release: bool) {
    if release {
        *vec = Vec::new();
        return;
    }

    for accelerator in vec {
        let map = accelerator.get_mut();
        if map.is_empty() {
            *map = FxHashMap::default();
        } else {
            map.clear();
        }
    }
}

/// Get an accelerator by ID.
pub fn get(id: usize) -> Option<MappedRwLockReadGuard<'static, Accelerator>> {
    // We always lock the accelerators, as we need to make sure that the
    // accelerator is not removed while we are reading it.
    loop {
        let accelerators = ACCELERATORS.read();
        let i = id.checked_sub(accelerators.0)?;
        if i < accelerators.1.len() {
            return Some(RwLockReadGuard::map(accelerators, move |(_, vec)| &vec[i]));
        }
        drop(accelerators);
        resize(id);
        // Eviction can change both the offset and the pool length while the
        // lock is released. Recheck both before indexing the pool.
    }
}

/// Adjusts the amount of accelerators.
#[cold]
fn resize(id: usize) {
    let mut pair = ACCELERATORS.write();
    let Some(i) = id.checked_sub(pair.0) else { return };
    let len = i + 1;
    if len > pair.1.len() {
        pair.1.resize_with(len, || Mutex::new(FxHashMap::default()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn populated() -> Accelerator {
        Mutex::new((0..128).map(|key| (key, key)).collect())
    }

    #[test]
    fn eviction_releases_idle_and_explicitly_cleared_storage() {
        let mut pool = (0..128).map(|_| populated()).collect::<Vec<_>>();
        evict_pool(&mut pool, false);
        assert!(pool[0].get_mut().is_empty());
        assert!(pool[0].get_mut().capacity() >= 128);

        // An unused epoch releases the empty tables, preserving pool slots.
        evict_pool(&mut pool, false);
        assert!(pool.iter_mut().all(|map| map.get_mut().capacity() == 0));
        pool[0].get_mut().insert(1, 2);
        evict_pool(&mut pool, true);
        assert!(pool.is_empty());
        assert_eq!(pool.capacity(), 0);
    }

    #[test]
    fn concurrent_access_and_eviction() {
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..2000 {
                        let id = id();
                        if let Some(accelerator) = get(id) {
                            let mut map = accelerator.lock();
                            map.insert(id as u128, 42);
                            assert_eq!(map.get(&(id as u128)), Some(&42));
                        }
                    }
                });
            }
            for _ in 0..2000 {
                evict(true);
                std::thread::yield_now();
            }
        });
        let expired = id();
        evict(true);
        assert!(get(expired).is_none());
    }
}
