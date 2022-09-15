use std::collections::HashMap;
use std::path::PathBuf;

use comemo::{Track, Tracked};

#[test]
fn test_script() {
    let mut storage = Storage::default();
    storage.store("alpha.calc", "2 + eval beta.calc");
    storage.store("beta.calc", "2 + 3");
    storage.store("gamma.calc", "8 + 3");

    // [Miss] The cache is empty.
    assert_eq!(eval_path(storage.track(), "alpha.calc"), 7);

    // [Hit]
    // This is a hit because `eval_path` was called recursively
    // with these arguments during the call above.
    assert_eq!(eval_path(storage.track(), "beta.calc"), 5);

    storage.store("gamma.calc", "8 + 3");

    // [Hit]
    // This is a top-level hit because `gamma.calc` isn't
    // referenced by `alpha.calc`.
    assert_eq!(eval_path(storage.track(), "alpha.calc"), 7);

    storage.store("beta.calc", "4 + eval gamma.calc");

    // [Miss]
    // This is a top-level miss because `beta.calc` changed.
    // However, parsing `alpha.calc` hits the cache.
    assert_eq!(eval_path(storage.track(), "alpha.calc"), 17);
}

/// File storage.
#[derive(Debug, Default)]
struct Storage {
    files: HashMap<PathBuf, String>,
}

impl Storage {
    /// Write a file to storage.
    fn store(&mut self, path: &str, text: &str) {
        self.files.insert(path.into(), text.into());
    }
}

#[comemo::track]
impl Storage {
    /// Load a file from storage.
    fn load(&self, path: PathBuf) -> String {
        self.files.get(&path).cloned().unwrap_or_default()
    }
}

/// An expression in the `.calc` language.
#[derive(Debug, Clone, Hash)]
enum Expr {
    /// A number.
    Number(i32),
    /// An `eval` expression with evaluates another file.
    Eval(String),
    /// A sum of other expressions: `1 + 2 + eval file.calc`.
    Sum(Vec<Expr>),
}

/// Evaluate a `.calc` script file at a path.
#[comemo::memoize]
fn eval_path(storage: Tracked<Storage>, path: &str) -> i32 {
    let file = storage.load(path.into());
    let expr = parse(&file);
    eval_expr(storage, &expr)
}

/// Evaluate a `.calc` expression.
#[comemo::memoize]
fn eval_expr(storage: Tracked<Storage>, expr: &Expr) -> i32 {
    match expr {
        Expr::Number(number) => *number,
        Expr::Eval(path) => eval_path(storage, path),
        Expr::Sum(exprs) => exprs.iter().map(|expr| eval_expr(storage, expr)).sum(),
    }
}

/// Parse a `.calc` script file.
#[comemo::memoize]
fn parse(src: &str) -> Expr {
    let mut s = unscanny::Scanner::new(src);
    let mut items = vec![];

    loop {
        s.eat_whitespace();

        let expr = if s.eat_if("eval") {
            s.eat_whitespace();
            let path = s.eat_while(|c: char| c == '.' || c.is_alphabetic());
            Expr::Eval(path.into())
        } else {
            let literal = s.eat_while(char::is_ascii_digit);
            Expr::Number(literal.parse().expect("expected number"))
        };

        items.push(expr);
        s.eat_whitespace();

        if s.done() {
            break;
        }

        s.expect('+');
    }

    if items.len() == 1 {
        items.into_iter().next().unwrap()
    } else {
        Expr::Sum(items)
    }
}
