use std::any::Any;
use std::cell::RefCell;
use std::fmt::Debug;

use crate::track::{from_parts, to_parts, Track, Trackable, Tracked};

thread_local! {
    /// The global, dynamic cache shared by all memoized functions.
    pub static CACHE: Cache = Cache::default();
}

/// An untyped cache.
#[derive(Default)]
pub struct Cache {
    map: RefCell<Vec<Box<dyn Any>>>,
}

/// An entry in the cache.
struct Entry<Tracker, R> {
    tracker: Tracker,
    output: R,
}

impl Cache {
    /// Execute `f` or use a cached result for it.
    pub fn query<F, T, R>(&self, name: &'static str, f: F, tracked: Tracked<T>) -> R
    where
        F: Fn(Tracked<T>) -> R,
        T: Track,
        R: Debug + Clone + 'static,
    {
        let mut hit = true;
        let output = self.lookup(tracked).unwrap_or_else(|| {
            let tracker = T::Tracker::default();
            let (inner, _) = to_parts(tracked);
            let tracked = from_parts(inner, Some(&tracker));
            let output = f(tracked);
            self.insert::<T, R>(tracker, output.clone());
            hit = false;
            output
        });

        let label = if hit { "[hit]: " } else { "[miss]:" };
        eprintln!("{name} {label} {output:?}");

        output
    }

    /// Look for a matching entry in the cache.
    fn lookup<T, R>(&self, tracked: Tracked<T>) -> Option<R>
    where
        T: Track,
        R: Clone + 'static,
    {
        let (inner, _) = to_parts(tracked);
        self.map
            .borrow()
            .iter()
            .filter_map(|boxed| boxed.downcast_ref::<Entry<T::Tracker, R>>())
            .find(|entry| Trackable::valid(inner, &entry.tracker))
            .map(|entry| entry.output.clone())
    }

    /// Insert an entry into the cache.
    fn insert<T, R>(&self, tracker: T::Tracker, output: R)
    where
        T: Track,
        R: 'static,
    {
        let entry = Entry { tracker, output };
        self.map.borrow_mut().push(Box::new(entry));
    }
}
