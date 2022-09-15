use comemo::Track;

#[test]
fn test_kinds() {
    let mut tester = Tester { data: "Hi".to_string() };
    let tracky = tester.track();

    unconditional(tracky); // [Miss] Never called.
    unconditional(tracky); // [Hit] Nothing changed.
    conditional(tracky, "World"); // [Miss] The cache is empty.
    ignorant(tracky, "Ignorant"); // [Miss] Never called.

    tester.data.push('!');

    let tracky = tester.track();
    unconditional(tracky); // [Miss] The combined length changed.
    conditional(tracky, "World"); // [Hit] "World" is still longer.
    ignorant(tracky, "Ignorant"); // [Hit] Doesn't depend on `tester`.

    tester.data.push_str(" Let's go.");

    let tracky = tester.track();
    unconditional(tracky); // [Miss] The combined length changed.
    conditional(tracky, "World"); // [Miss] "World" is now shorter.
    ignorant(tracky, "Ignorant"); // [Hit] Doesn't depend on `tester`.
}

/// Always accesses data from both arguments.
#[comemo::memoize]
fn unconditional(tester: Tracky) -> &'static str {
    if tester.by_value(Heavy("HEAVY".into())) > 10 {
        "Long"
    } else {
        "Short"
    }
}

/// Accesses data from both arguments conditionally.
#[comemo::memoize]
fn conditional(tester: Tracky, name: &str) -> String {
    tester.double_ref(name).to_string()
}

/// Accesses only data from the second argument.
#[comemo::memoize]
fn ignorant(tester: Tracky, name: &str) -> String {
    tester.arg_ref(name).to_string()
}

/// Test with type alias.
type Tracky<'a> = comemo::Tracked<'a, Tester>;

/// A struct with some data.
struct Tester {
    data: String,
}

#[comemo::track]
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
#[derive(Clone, PartialEq)]
struct Heavy(String);
