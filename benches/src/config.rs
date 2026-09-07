//! Build-profile guards shared by the benchmark targets.

/// Refuses to run in a debug build.
///
/// Without inlining and vectorisation the numbers are not wrong so much as unrelated to what the
/// libraries do in a real build, and they look every bit as real as the ones that matter. For the
/// binaries, where the user picks the profile, that is worth stopping over.
pub fn require_release_build()
{
    if cfg!(debug_assertions)
    {
        eprintln!("error: this is a debug build, and its numbers would not mean anything.");
        eprintln!("       run `cargo run --release -p xynok_ecs_benches --bin report` instead.");
        std::process::exit(1);
    }
}

/// The same check for the criterion target, which only warns.
///
/// `cargo bench` always builds with optimisations, so a debug build here means `cargo test
/// --benches`, where criterion runs one iteration of each benchmark purely to check it still
/// compiles and does not panic. Exiting would turn that useful check into a failing test suite.
pub fn warn_if_debug_build()
{
    if cfg!(debug_assertions)
    {
        eprintln!("warning: debug build, timings from this run are not meaningful. Use `cargo bench`.");
    }
}
