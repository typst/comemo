use std::path::Path;

use comemo::{Track, Tracked};

#[test]
fn test_trait() {
    let loader: &dyn Loader = &StaticLoader;
    load_file(loader.track(), Path::new("hi.rs"));
    load_file(loader.track(), Path::new("hi.rs"));
    load_file(loader.track(), Path::new("bye.rs"));
}

/// Load a file from the loader.
#[comemo::memoize]
fn load_file(loader: Tracked<dyn Loader>, path: &Path) -> Vec<u8> {
    loader.load(path).unwrap()
}

/// A trait for which the trait object's methods are tracked.
#[comemo::track]
trait Loader {
    fn load(&self, path: &Path) -> Result<Vec<u8>, String>;
}

struct StaticLoader;

impl Loader for StaticLoader {
    fn load(&self, _: &Path) -> Result<Vec<u8>, String> {
        Ok(vec![1, 2, 3])
    }
}
