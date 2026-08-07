//#![allow(unused)]

// The `#[component]` macro generates an absolute path like `xynok_ecs::...`.
// Without this alias, when the macro is used within the crate itself,
// `xynok_ecs` points to the dev-dependency `xynok_ecs = { path = "." }`.
// This creates a separate compilation, causing the generated trait
// to mismatch the trait in the current crate.
extern crate self as xynok_ecs;

pub mod apis;
pub mod world;
pub mod query;
pub mod entity;
pub mod schedule;
pub mod cmd_buffer;

mod utils;
mod chunk;
mod archetype;
mod system;
mod collection;

pub use xynok_ecs_proc_macro::*;
