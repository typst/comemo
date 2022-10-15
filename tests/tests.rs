use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use comemo::{evict, memoize, track, Track, Tracked};

macro_rules! test {
    (miss: $call:expr, $result:expr) => {{
        assert_eq!($call, $result);
        assert!(!comemo::internal::last_was_hit());
    }};
    (hit: $call:expr, $result:expr) => {{
        assert_eq!($call, $result);
        assert!(comemo::internal::last_was_hit());
    }};
}

/// Test basic memoization.
#[test]
fn test_basic() {
    #[memoize]
    fn empty() -> String {
        format!("The world is {}", "big")
    }

    #[memoize]
    fn double(x: u32) -> u32 {
        2 * x
    }

    #[memoize]
    fn sum(a: u32, b: u32) -> u32 {
        a + b
    }

    test!(miss: empty(), "The world is big");
    test!(hit: empty(), "The world is big");
    test!(hit: empty(), "The world is big");

    test!(miss: double(2), 4);
    test!(miss: double(4), 8);
    test!(hit: double(2), 4);

    test!(miss: sum(2, 4), 6);
    test!(miss: sum(2, 3), 5);
    test!(hit: sum(2, 3), 5);
    test!(miss: sum(4, 2), 6);
}

/// Test the calc language.
#[test]
fn test_calc() {
    #[memoize]
    fn evaluate(script: &str, files: Tracked<Files>) -> i32 {
        script
            .split('+')
            .map(str::trim)
            .map(|part| match part.strip_prefix("eval ") {
                Some(path) => evaluate(&files.read(path), files),
                None => part.parse::<i32>().unwrap(),
            })
            .sum()
    }

    let mut files = Files(HashMap::new());
    files.write("alpha.calc", "2 + eval beta.calc");
    files.write("beta.calc", "2 + 3");
    files.write("gamma.calc", "8 + 3");
    test!(miss: evaluate("eval alpha.calc", files.track()), 7);
    test!(miss: evaluate("eval beta.calc", files.track()), 5);
    files.write("gamma.calc", "42");
    test!(hit: evaluate("eval alpha.calc", files.track()), 7);
    files.write("beta.calc", "4 + eval gamma.calc");
    test!(miss: evaluate("eval beta.calc", files.track()), 46);
    test!(miss: evaluate("eval alpha.calc", files.track()), 48);
    files.write("gamma.calc", "80");
    test!(miss: evaluate("eval alpha.calc", files.track()), 86);
}

struct Files(HashMap<PathBuf, String>);

#[track]
impl Files {
    fn read(&self, path: &str) -> String {
        self.0.get(Path::new(path)).cloned().unwrap_or_default()
    }
}

impl Files {
    fn write(&mut self, path: &str, text: &str) {
        self.0.insert(path.into(), text.into());
    }
}

/// Test cache eviction.
#[test]
fn test_evict() {
    #[memoize]
    fn null() -> u8 {
        0
    }

    test!(miss: null(), 0);
    test!(hit: null(), 0);
    evict(2);
    test!(hit: null(), 0);
    evict(2);
    evict(2);
    test!(hit: null(), 0);
    evict(2);
    evict(2);
    evict(2);
    test!(miss: null(), 0);
    test!(hit: null(), 0);
    evict(0);
    test!(miss: null(), 0);
    test!(hit: null(), 0);
}

/// Test tracking a trait object.
#[test]
fn test_tracked_trait() {
    #[memoize]
    fn traity(loader: Tracked<dyn Loader>, path: &Path) -> Vec<u8> {
        loader.load(path).unwrap()
    }

    let loader: &dyn Loader = &StaticLoader;
    test!(miss: traity(loader.track(), Path::new("hi.rs")), [1, 2, 3]);
    test!(hit: traity(loader.track(), Path::new("hi.rs")), [1, 2, 3]);
    test!(miss: traity(loader.track(), Path::new("bye.rs")), [1, 2, 3]);
}

#[track]
trait Loader {
    fn load(&self, path: &Path) -> Result<Vec<u8>, String>;
}

struct StaticLoader;
impl Loader for StaticLoader {
    fn load(&self, _: &Path) -> Result<Vec<u8>, String> {
        Ok(vec![1, 2, 3])
    }
}

/// Test memoized methods.
#[test]
fn test_memoized_methods() {
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

    test!(miss: Taker("Hello".into()).take(), "Hello");
    test!(miss: Taker("Hello".into()).copy(), "Hello");
    test!(miss: Taker("World".into()).take(), "World");
    test!(hit: Taker("Hello".into()).take(), "Hello");
}

/// Test different kinds of arguments.
#[test]
fn test_kinds() {
    #[memoize]
    fn selfie(tester: Tracky) -> String {
        tester.self_ref().into()
    }

    #[memoize]
    fn unconditional(tester: Tracky) -> &'static str {
        if tester.by_value(Heavy("HEAVY".into())) > 10 {
            "Long"
        } else {
            "Short"
        }
    }

    #[memoize]
    fn generic<T>(tester: Tracky, name: T) -> String
    where
        T: AsRef<str> + Hash,
    {
        tester.double_ref(name.as_ref()).to_string()
    }

    #[memoize]
    fn ignorant(tester: Tracky, name: impl AsRef<str> + Hash) -> String {
        tester.arg_ref(name.as_ref()).to_string()
    }

    let mut tester = Tester { data: "Hi".to_string() };

    let tracky = tester.track();
    test!(miss: selfie(tracky), "Hi");
    test!(miss: unconditional(tracky), "Short");
    test!(hit: unconditional(tracky), "Short");
    test!(miss: generic(tracky, "World"), "World");
    test!(miss: ignorant(tracky, "Ignorant"), "Ignorant");
    test!(hit: selfie(tracky), "Hi");

    tester.data.push('!');

    let tracky = tester.track();
    test!(miss: selfie(tracky), "Hi!");
    test!(miss: unconditional(tracky), "Short");
    test!(hit: generic(tracky, "World"), "World");
    test!(hit: ignorant(tracky, "Ignorant"), "Ignorant");

    tester.data.push_str(" Let's go.");

    let tracky = tester.track();
    test!(miss: unconditional(tracky), "Long");
    test!(miss: generic(tracky, "World"), "Hi! Let's go.");
    test!(hit: ignorant(tracky, "Ignorant"), "Ignorant");
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

/// Test a tracked method that is impure.
#[test]
#[should_panic(
    expected = "comemo: found conflicting constraints. is this tracked function pure?"
)]
fn test_impure_tracked_method() {
    #[comemo::memoize]
    fn call(impure: Tracked<Impure>) -> u32 {
        impure.impure();
        impure.impure()
    }

    call(Impure.track());
}

struct Impure;

#[track]
impl Impure {
    fn impure(&self) -> u32 {
        use std::sync::atomic::{AtomicU32, Ordering};
        static VAL: AtomicU32 = AtomicU32::new(0);
        VAL.fetch_add(1, Ordering::SeqCst)
    }
}
