//! This example demonstrates how to memoize the execution of scripts which can
//! depend on other scripts---invalidating the result of a script's execution
//! only if a file it depends on changes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use comemo::{memoize, track, Track, Tracked};

fn main() {
    // Create some scripts in the calc language. This language supports addition
    // and `eval` statements referring to other files.
    let mut files = Files(HashMap::new());
    files.write("alpha.calc", "2 + eval beta.calc");
    files.write("beta.calc", "2 + 3");
    files.write("gamma.calc", "8 + 3");

    // [Miss] The cache is empty.
    assert_eq!(evaluate("eval alpha.calc", files.track()), 7);

    // [Miss] This is not a top-level hit because this exact string was never
    // passed to `evaluate`, but this does not compute "2 + 3" again.
    assert_eq!(evaluate("eval beta.calc", files.track()), 5);

    // Modify the gamma file.
    files.write("gamma.calc", "42");

    // [Hit] This is a hit because `gamma.calc` isn't referenced by `alpha.calc`.
    assert_eq!(evaluate("eval alpha.calc", files.track()), 7);

    // Modify the beta file.
    files.write("beta.calc", "4 + eval gamma.calc");

    // [Miss] This is a miss because `beta.calc` changed.
    assert_eq!(evaluate("eval alpha.calc", files.track()), 48);
}

/// Evaluate a `.calc` script.
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

/// File storage.
struct Files(HashMap<PathBuf, String>);

#[track]
impl Files {
    /// Read a file from storage.
    fn read(&self, path: &str) -> String {
        self.0.get(Path::new(path)).cloned().unwrap_or_default()
    }
}

impl Files {
    /// Write a file to storage.
    fn write(&mut self, path: &str, text: &str) {
        self.0.insert(path.into(), text.into());
    }
}
