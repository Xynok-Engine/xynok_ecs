//#![allow(unused)]

// `#[component]` sinh ra đường dẫn tuyệt đối `xynok_ecs::...`. Nếu không có alias này,
// khi macro được dùng ngay trong chính crate thì `xynok_ecs` sẽ trỏ tới bản
// dev-dependency `xynok_ecs = { path = "." }` — một bản compile riêng — khiến trait
// sinh ra không khớp với trait của crate hiện tại.
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

pub use xynok_ecs_proc_macro::*;
