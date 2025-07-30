//! Run with `cargo test --all-features`.

use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;

use comemo::{Track, Tracked, TrackedMut, evict, memoize, track};
use quickcheck::Arbitrary;
use serial_test::serial;

macro_rules! test {
    (miss: $call:expr, $result:expr) => {{
        assert_eq!($call, $result);
        assert!(!comemo::testing::last_was_hit());
    }};
    (hit: $call:expr, $result:expr) => {{
        assert_eq!($call, $result);
        assert!(comemo::testing::last_was_hit());
    }};
}

/// Test basic memoization.
#[test]
#[serial]
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

    #[memoize]
    fn fib(n: u32) -> u32 {
        if n <= 2 { 1 } else { fib(n - 1) + fib(n - 2) }
    }

    #[memoize]
    fn sum_iter(n: u32) -> u32 {
        (0..n).sum()
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

    test!(miss: fib(5), 5);
    test!(hit: fib(3), 2);
    test!(miss: fib(8), 21);
    test!(hit: fib(7), 13);

    test!(miss: sum_iter(1000), 499500);
    test!(hit: sum_iter(1000), 499500);
}

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

/// Test the calc language.
#[test]
#[serial]
fn test_calc() {
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
#[serial]
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
#[serial]
fn test_tracked_trait() {
    #[memoize]
    fn traity(loader: Tracked<dyn Loader + '_>, path: &Path) -> Vec<u8> {
        loader.load(path).unwrap()
    }

    fn wrapper(loader: &(dyn Loader), path: &Path) -> Vec<u8> {
        traity(loader.track(), path)
    }

    let loader: &(dyn Loader) = &StaticLoader;
    test!(miss: traity(loader.track(), Path::new("hi.rs")), [1, 2, 3]);
    test!(hit: traity(loader.track(), Path::new("hi.rs")), [1, 2, 3]);
    test!(miss: traity(loader.track(), Path::new("bye.rs")), [1, 2, 3]);
    wrapper(loader, Path::new("hi.rs"));
}

#[track]
trait Loader: Send + Sync {
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
#[serial]
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
#[serial]
fn test_kinds() {
    #[memoize]
    fn selfie(tester: Tracky) -> String {
        tester.self_ref().into()
    }

    #[memoize]
    fn unconditional(tester: Tracky) -> &'static str {
        if tester.by_value(Heavy("HEAVY".into())) > 10 { "Long" } else { "Short" }
    }

    let mut tester = Tester { data: "Hi".to_string() };

    let tracky = tester.track();
    test!(miss: selfie(tracky), "Hi");
    test!(miss: unconditional(tracky), "Short");
    test!(hit: unconditional(tracky), "Short");
    test!(hit: selfie(tracky), "Hi");

    tester.data.push('!');

    let tracky = tester.track();
    test!(miss: selfie(tracky), "Hi!");
    test!(miss: unconditional(tracky), "Short");

    tester.data.push_str(" Let's go.");

    let tracky = tester.track();
    test!(miss: unconditional(tracky), "Long");
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
    #[allow(clippy::needless_lifetimes)]
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
#[derive(Clone, PartialEq, Hash)]
struct Heavy(String);

/// Test empty type without methods.
struct Empty;

#[track]
impl Empty {}

/// Test tracking a type with a lifetime.
#[test]
#[serial]
fn test_lifetime() {
    #[comemo::memoize]
    fn contains_hello(lifeful: Tracked<Lifeful>) -> bool {
        lifeful.contains("hello")
    }

    let lifeful = Lifeful("hey");
    test!(miss: contains_hello(lifeful.track()), false);
    test!(hit: contains_hello(lifeful.track()), false);

    let lifeful = Lifeful("hello");
    test!(miss: contains_hello(lifeful.track()), true);
    test!(hit: contains_hello(lifeful.track()), true);
}

/// Test tracked with lifetime.
struct Lifeful<'a>(&'a str);

#[track]
impl<'a> Lifeful<'a> {
    fn contains(&self, text: &str) -> bool {
        self.0 == text
    }
}

/// Test tracking a type with a chain of tracked values.
#[test]
#[serial]
fn test_chain() {
    #[comemo::memoize]
    fn process(chain: Tracked<Chain>, value: u32) -> bool {
        chain.contains(value)
    }

    let chain1 = Chain::new(1);
    let chain3 = Chain::new(3);
    let chain12 = Chain::insert(chain1.track(), 2);
    let chain123 = Chain::insert(chain12.track(), 3);
    let chain124 = Chain::insert(chain12.track(), 4);
    let chain1245 = Chain::insert(chain124.track(), 5);

    test!(miss: process(chain1.track(), 0), false);
    test!(miss: process(chain1.track(), 1), true);
    test!(miss: process(chain123.track(), 2), true);
    test!(hit: process(chain124.track(), 2), true);
    test!(hit: process(chain12.track(), 2), true);
    test!(hit: process(chain1245.track(), 2), true);
    test!(miss: process(chain1.track(), 2), false);
    test!(hit: process(chain3.track(), 2), false);
}

/// Test that `Tracked<T>` is covariant over `T`.
#[test]
#[serial]
#[allow(unused, clippy::needless_lifetimes)]
fn test_variance() {
    fn foo<'a>(_: Tracked<'a, Chain<'a>>) {}
    fn bar<'a>(chain: Tracked<'a, Chain<'static>>) {
        foo(chain);
    }
}

/// Test tracked with lifetime.
struct Chain<'a> {
    // Need to override the lifetime here so that a `Tracked` is covariant over
    // `Chain`.
    outer: Option<Tracked<'a, Self, <Chain<'static> as Track>::Call>>,
    value: u32,
}

impl<'a> Chain<'a> {
    /// Create a new chain entry point.
    fn new(value: u32) -> Self {
        Self { outer: None, value }
    }

    /// Insert a link into the chain.
    fn insert(outer: Tracked<'a, Self>, value: u32) -> Self {
        Chain { outer: Some(outer), value }
    }
}

#[track]
impl<'a> Chain<'a> {
    fn contains(&self, value: u32) -> bool {
        self.value == value || self.outer.is_some_and(|outer| outer.contains(value))
    }
}

/// Test purely mutable tracking.
#[test]
#[serial]
#[rustfmt::skip]
fn test_purely_mutable() {
    #[comemo::memoize]
    fn dump(mut emitter: TrackedMut<Emitter>, value: &str) {
        emitter.emit(value);
        emitter.emit("1");
    }

    let mut emitter = Emitter(vec![]);
    test!(miss: dump(emitter.track_mut(), "a"), ());
    test!(miss: dump(emitter.track_mut(), "b"), ());
    test!(miss: dump(emitter.track_mut(), "c"), ());
    test!(hit: dump(emitter.track_mut(), "a"), ());
    test!(hit: dump(emitter.track_mut(), "b"), ());
    assert_eq!(emitter.0, [
        "a", "1",
        "b", "1",
        "c", "1",
        "a", "1",
        "b", "1",
    ])
}

/// A tracked type with a mutable and an immutable method.
#[derive(Clone)]
struct Emitter(Vec<String>);

#[track]
impl Emitter {
    fn emit(&mut self, msg: &str) {
        self.0.push(msg.into());
    }
}

/// Ensures that we don't run into quadratic runtime during cache validation of
/// many cache entries with the same key hash.
#[test]
#[serial]
fn test_many_with_same_key() {
    #[memoize]
    fn contextual(context: Tracked<Context>) -> String {
        if let Some(loc) = context.location() {
            if loc == 5 {
                format!("Twenty has {}", context.styles())
            } else {
                format!("Location: {loc}")
            }
        } else {
            "No location".into()
        }
    }

    fn oracle(context: &Context) -> String {
        if let Some(loc) = context.location {
            if loc == 5 {
                format!("Twenty has {}", context.styles)
            } else {
                format!("Location: {loc}")
            }
        } else {
            "No location".into()
        }
    }

    for i in 0..1000 {
        let context = Context { location: Some(i), styles: "styles" };
        test!(miss: contextual(context.track()), oracle(&context));
    }

    for i in 0..1000 {
        let context = Context { location: Some(i), styles: "styles" };
        test!(hit: contextual(context.track()), oracle(&context));
    }
}

/// Tests a memoized function that calls tracked functions in non-deterministic
/// fashion. (Not just out of order, but some call that appeared in one run does
/// not appear at all in the other even though the same calls and return hashes
/// led up to that point.q)
#[test]
#[serial]
#[should_panic(expected = "comemo: memoized function is non-deterministic")]
fn test_non_deterministic() {
    use std::sync::atomic::Ordering::SeqCst;

    static FOO: AtomicUsize = AtomicUsize::new(0);

    #[memoize]
    fn contextual(context: Tracked<Context>) -> String {
        if FOO.load(SeqCst) == 0 {
            let _ = context.location();
        } else {
            let _ = context.styles();
        }
        String::new()
    }

    let context = Context { location: Some(0), styles: "styles" };
    FOO.store(0, SeqCst);
    contextual(context.track());

    let context = Context { location: Some(1), styles: "styles" };
    FOO.store(1, SeqCst);
    contextual(context.track());
}

/// Tests a memoized function that calls tracked functions out of order, but in
/// a fashion that is still deterministic in which functions are called overall
/// (this happens in deterministic functions that use multi-threading
/// internally).
#[test]
#[serial]
fn test_deterministic_out_of_order() {
    use std::sync::atomic::Ordering::SeqCst;

    static FOO: AtomicUsize = AtomicUsize::new(0);

    #[memoize]
    fn contextual(context: Tracked<Context>) -> String {
        let (a, b) = if FOO.load(SeqCst) == 0 {
            let a = context.location();
            let b = context.styles();
            (a, b)
        } else {
            let b = context.styles();
            let a = context.location();
            (a, b)
        };
        format!("{a:?} {b}")
    }

    let context = Context { location: Some(0), styles: "styles" };
    FOO.store(0, SeqCst);
    test!(miss: contextual(context.track()), "Some(0) styles");

    FOO.store(1, SeqCst);
    test!(hit: contextual(context.track()), "Some(0) styles");

    let context = Context { location: Some(1), styles: "styles" };
    test!(miss: contextual(context.track()), "Some(1) styles");
}

struct Context {
    location: Option<u64>,
    styles: &'static str,
}

#[track]
impl Context {
    fn location(&self) -> Option<u64> {
        self.location
    }

    fn styles(&self) -> &'static str {
        self.styles
    }
}

/// Test a tracked method that is impure.
#[test]
#[serial]
#[cfg(debug_assertions)]
#[should_panic(
    expected = "comemo: found differing return values. is there an impure tracked function?"
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

#[test]
#[serial]
#[cfg(debug_assertions)]
fn test_with_disabled() {
    #[comemo::memoize(enabled = size >= 1000)]
    fn disabled(size: usize) -> usize {
        size
    }

    test!(miss: disabled(0), 0);
    test!(miss: disabled(0), 0);

    test!(miss: disabled(2000), 2000);
    test!(hit: disabled(2000), 2000);
}

#[quickcheck_macros::quickcheck]
fn test_memoize_quickcheck(cases: Cases) {
    for Case(map, tree) in cases.0 {
        let mut c1 = Counter(0);
        let r1 = fuzzable_unmemoized(&map, &mut c1, &tree);

        let mut c2 = Counter(0);
        let r2 = fuzzable(map.track(), c2.track_mut(), &tree);

        let mut c3 = Counter(0);
        let r3 = fuzzable(map.track(), c3.track_mut(), &tree);
        assert!(comemo::testing::last_was_hit());

        assert_eq!(r1, r2);
        assert_eq!(r2, r3);
        assert_eq!(c1, c2);
        assert_eq!(c2, c3);
    }
    comemo::evict(2)
}

#[memoize]
fn fuzzable(
    map: Tracked<IntMap>,
    mut counter: TrackedMut<Counter>,
    tree: &[Node],
) -> u32 {
    tree.iter()
        .filter_map(|node| match node {
            Node::Leaf(leaf) => {
                if *leaf == 7 {
                    counter.add(1);
                }
                map.get(*leaf)
            }
            Node::Inner(inner, _) => {
                Some(fuzzable(map, TrackedMut::reborrow_mut(&mut counter), inner))
            }
        })
        .fold(0, |a, b| a.saturating_add(b))
}

fn fuzzable_unmemoized(map: &IntMap, counter: &mut Counter, tree: &[Node]) -> u32 {
    tree.iter()
        .filter_map(|node| match node {
            Node::Leaf(leaf) => {
                if *leaf == 7 {
                    counter.add(1);
                }
                map.get(*leaf)
            }
            Node::Inner(inner, _) => Some(fuzzable_unmemoized(map, counter, inner)),
        })
        .fold(0, |a, b| a.saturating_add(b))
}

#[derive(Debug, Clone)]
struct Cases(Vec<Case>);

impl Arbitrary for Cases {
    fn arbitrary(_: &mut quickcheck::Gen) -> Self {
        Self(Arbitrary::arbitrary(&mut quickcheck::Gen::new(5)))
    }
}

#[derive(Debug, Clone)]
struct Case(IntMap, Vec<Node>);

impl Arbitrary for Case {
    fn arbitrary(_: &mut quickcheck::Gen) -> Self {
        let g = &mut quickcheck::Gen::new(100);
        Self(Arbitrary::arbitrary(g), Arbitrary::arbitrary(g))
    }
}

#[derive(Debug, Clone, Hash)]
enum Node {
    Leaf(u32),
    Inner(Vec<Node>, usize),
}

impl Node {
    fn depth(&self) -> usize {
        match self {
            Self::Leaf(_) => 0,
            Self::Inner(_, depth) => *depth,
        }
    }
}

impl Arbitrary for Node {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        if g.size() == 0 || bool::arbitrary(g) {
            Self::Leaf(Arbitrary::arbitrary(g))
        } else {
            let g = &mut quickcheck::Gen::new(g.size() / 3);
            let nodes: Vec<Node> = Arbitrary::arbitrary(g);
            let depth = nodes.iter().map(|node| node.depth() + 1).max().unwrap_or(0);
            Self::Inner(nodes, depth)
        }
    }
}

#[derive(Debug, Clone)]
struct IntMap(HashMap<u32, u32>);

impl Arbitrary for IntMap {
    fn arbitrary(g: &mut quickcheck::Gen) -> Self {
        Self(Arbitrary::arbitrary(g))
    }
}

#[track]
impl IntMap {
    fn get(&self, k: u32) -> Option<u32> {
        self.0.get(&k).copied()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct Counter(usize);

#[track]
impl Counter {
    fn add(&mut self, v: usize) {
        self.0 += v;
    }
}
