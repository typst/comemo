//! This example demonstrates super basic memoization without any tracking.
//! While comemo goes way beyond this, it's of course also possible!

use comemo::memoize;

fn main() {
    empty(); // [Miss] The cache is empty.
    empty(); // [Hit] Always a hit from now on.
    empty(); // [Hit] Always a hit from now on.

    double(2); // [Miss] The cache is empty.
    double(4); // [Miss] Different number.
    double(2); // [Hit] Same number as initially.

    sum(2, 4); // [Miss] The cache is empty.
    sum(2, 3); // [Miss] Different numbers.
    sum(2, 3); // [Hit]  Same numbers
    sum(4, 2); // [Miss] Different numbers.
}

/// Build a string.
#[memoize]
fn empty() -> String {
    format!("The world is {}", "big")
}

/// Double a number.
#[memoize]
fn double(x: u32) -> u32 {
    2 * x
}

/// Compute the sum of two numbers.
#[memoize]
fn sum(a: u32, b: u32) -> u32 {
    a + b
}
