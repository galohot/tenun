//! Tenun's engine: sources, relationships and the query compiler, on DuckDB.

pub mod advanced;
pub mod catalog;
pub mod engine;
pub mod excel;
pub mod fetch;
pub mod filters;
pub mod folder;
pub mod formula;
pub mod jsonrec;
pub mod links;
pub mod paste;
pub mod prep;
pub mod query;
pub mod secrets;
pub mod shape;
pub mod snapshot;
pub mod sqlite;
pub mod suggest;
pub mod watch;
pub mod workspace;

pub use engine::Engine;
