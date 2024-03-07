# comemo
[![Crates.io](https://img.shields.io/crates/v/comemo.svg)](https://crates.io/crates/comemo)
[![Documentation](https://docs.rs/comemo/badge.svg)](https://docs.rs/comemo)

Incremental computation through constrained memoization.

```toml
[dependencies]
comemo = "0.4"
```

A _memoized_ function caches its return values so that it only needs to be
executed once per set of unique arguments. This makes for a great optimization
tool. However, basic memoization is rather limited. For more advanced use cases
like incremental compilers, it lacks the necessary granularity. Consider, for
example, the case of the simple `.calc` scripting language. Scripts in this
language consist of a sum of numbers and `eval` statements that reference other
`.calc` scripts. A few examples are:

- `alpha.calc`: `"2 + eval beta.calc"`
- `beta.calc`: `"2 + 3"`
- `gamma.calc`: `"8 + 3"`

We can easily write an interpreter that computes the output of a `.calc` file:

```rust
/// Evaluate a `.calc` script.
fn evaluate(script: &str, files: &Files) -> i32 {
    script
        .split('+')
        .map(str::trim)
        .map(|part| match part.strip_prefix("eval ") {
            Some(path) => evaluate(&files.read(path), files),
            None => part.parse::<i32>().unwrap(),
        })
        .sum()
}

impl Files {
    /// Read a file from storage.
    fn read(&self, path: &str) -> String {
        ...
    }
}
```

But what if we want to make this interpreter _incremental,_ meaning that it only
recomputes a script's result if it or any of its dependencies change? Basic
memoization won't help us with this because the interpreter needs the whole set
of files as input—meaning that a change to any file invalidates all memoized
results.

This is where comemo comes into play. It implements _constrained memoization_
with more fine-grained access tracking. To use it, we can just:

- Add the `#[memoize]` attribute to the `evaluate` function.
- Add the `#[track]` attribute to the impl block of `Files`.
- Wrap the `files` argument in comemo's `Tracked` container.

This instructs comemo to memoize the evaluation and to automatically track all
file accesses during a memoized call. As a result, we can reuse the result of a
`.calc` script evaluation as long as its dependencies stay the same—even if
other files change.

```rust
use comemo::{memoize, track, Tracked};

/// Evaluate a `.calc` script.
#[memoize]
fn evaluate(script: &str, files: Tracked<Files>) -> i32 {
    ...
}

#[track]
impl Files {
    /// Read a file from storage.
    fn read(&self, path: &str) -> String {
        ...
    }
}
```

For the full example see [`examples/calc.rs`][calc].

[calc]: https://github.com/typst/comemo/blob/main/examples/calc.rs

## License
This crate is dual-licensed under the MIT and Apache 2.0 licenses.
