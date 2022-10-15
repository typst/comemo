use std::hash::Hash;
use std::path::Path;

use comemo::{memoize, track, Track, Tracked};

#[test]
fn test_kinds() {
    let mut tester = Tester { data: "Hi".to_string() };

    let tracky = tester.track();
    unconditional(tracky); // [Miss] Never called.
    unconditional(tracky); // [Hit] Nothing changed.
    generic(tracky, "World"); // [Miss] The cache is empty.
    ignorant(tracky, "Ignorant"); // [Miss] Never called.

    tester.data.push('!');

    let tracky = tester.track();
    unconditional(tracky); // [Miss] The combined length changed.
    generic(tracky, "World"); // [Hit] "World" is still longer.
    ignorant(tracky, "Ignorant"); // [Hit] Doesn't depend on `tester`.

    tester.data.push_str(" Let's go.");

    let tracky = tester.track();
    unconditional(tracky); // [Miss] The combined length changed.
    generic(tracky, "World"); // [Miss] "World" is now shorter.
    ignorant(tracky, "Ignorant"); // [Hit] Doesn't depend on `tester`.
}

#[test]
fn test_memoized_methods() {
    Taker("Hello".into()).take(); // [Miss] Never called.
    Taker("Hello".into()).copy(); // [Miss] Never called.
    Taker("World".into()).take(); // [Miss] Different value.
    Taker("Hello".into()).take(); // [Hit] Same value.
}

#[test]
fn test_tracked_trait() {
    let loader: &dyn Loader = &StaticLoader;
    traity(loader.track(), Path::new("hi.rs")); // [Miss] Never called.
    traity(loader.track(), Path::new("hi.rs")); // [Hit] Stayed the same.
    traity(loader.track(), Path::new("bye.rs")); // [Miss] Different path.
}

/// Always accesses data from both arguments.
#[memoize]
fn unconditional(tester: Tracky) -> &'static str {
    if tester.by_value(Heavy("HEAVY".into())) > 10 {
        "Long"
    } else {
        "Short"
    }
}

/// Accesses data from both arguments conditionally.
#[memoize]
fn generic<T>(tester: Tracky, name: T) -> String
where
    T: AsRef<str> + Hash,
{
    tester.double_ref(name.as_ref()).to_string()
}

/// Accesses only data from the second argument.
#[memoize]
fn ignorant(tester: Tracky, name: impl AsRef<str> + Hash) -> String {
    tester.arg_ref(name.as_ref()).to_string()
}

/// Has a tracked trait object argument.
#[memoize]
fn traity(loader: Tracked<dyn Loader>, path: &Path) -> Vec<u8> {
    loader.load(path).unwrap()
}

/// Test with type alias.
type Tracky<'a> = comemo::Tracked<'a, Tester>;

/// A struct with some data.
struct Tester {
    data: String,
}

/// Tests different kinds of arguments.
#[track]
impl Tester {
    /// Return value can borrow from self.
    fn self_ref<'a>(&'a self) -> &'a str {
        &self.data
    }

    /// Return value can borrow from argument.
    fn arg_ref<'a>(&self, name: &'a str) -> &'a str {
        name
    }

    /// Return value can borrow from both.
    fn double_ref<'a>(&'a self, name: &'a str) -> &'a str {
        if name.len() > self.data.len() { name } else { &self.data }
    }

    /// Normal method with owned argument.
    fn by_value(&self, heavy: Heavy) -> usize {
        self.data.len() + heavy.0.len()
    }
}

/// A non-copy struct that is passed by value to a tracked method.
#[derive(Debug, Clone, PartialEq)]
struct Heavy(String);

#[derive(Hash)]
struct Taker(String);

/// Has memoized methods.
impl Taker {
    #[memoize]
    fn copy(&self) -> String {
        self.0.clone()
    }

    #[memoize]
    fn take(self) -> String {
        self.0
    }
}

/// A trait for which the trait object's methods are tracked.
#[track]
trait Loader {
    fn load(&self, path: &Path) -> Result<Vec<u8>, String>;
}

/// Trait implementor.
struct StaticLoader;

impl Loader for StaticLoader {
    fn load(&self, _: &Path) -> Result<Vec<u8>, String> {
        Ok(vec![1, 2, 3])
    }
}
