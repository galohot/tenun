//! The list of functions the editor offers (`ui/src/lib/functions.json`) is the list the engine
//! has: same names, same kind, same number of arguments. Two copies of a rule drift, so both are
//! tested against each other (build 02).

use serde::Deserialize;
use tenun_core::formula::functions::{Class, DEFS};

const LIST: &str = include_str!("../../../ui/src/lib/functions.json");

#[derive(Deserialize)]
struct Entry {
    name: String,
    args: String,
    class: String,
    hint: String,
}

/// The least and the most arguments a signature like `x, [n]` or `test, value, …` takes.
fn arity(args: &str) -> (usize, usize) {
    let parts: Vec<&str> = args.split(", ").filter(|p| !p.is_empty()).collect();
    let many = parts.contains(&"…");
    let named: Vec<&&str> = parts.iter().filter(|p| **p != "…").collect();
    let min = named.iter().filter(|p| !p.starts_with('[')).count();
    let max = if many { usize::MAX } else { named.len() };
    (min, max)
}

#[test]
fn the_editor_offers_exactly_the_functions_the_engine_has() {
    let list: Vec<Entry> = serde_json::from_str(LIST).unwrap();
    let names: Vec<&str> = list.iter().map(|e| e.name.as_str()).collect();
    let engine: Vec<&str> = DEFS.iter().map(|d| d.name).collect();
    assert_eq!(names, engine);
    for (e, d) in list.iter().zip(DEFS) {
        let class = match d.class {
            Class::Row => "row",
            Class::Agg => "agg",
            Class::Either => "either",
        };
        assert_eq!(e.class, class, "{}", e.name);
        let (min, max) = arity(&e.args);
        assert_eq!((min, max), (d.min, d.max), "{}({})", e.name, e.args);
        assert!(
            !e.hint.is_empty() && e.hint.len() <= 80,
            "{}: {}",
            e.name,
            e.hint
        );
    }
}
