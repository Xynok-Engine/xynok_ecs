//! Single-threaded query iteration: `xynok_ecs` against `bevy_ecs`, with a plain `Vec` as the floor.
//!
//! ```bash
//! cargo bench -p xynok_ecs_benches --bench query
//! open target/criterion/report/index.html
//! ```
//!
//! One benchmark is one full pass over every matching entity. The storage and the query are built
//! once, before the timed region, and reused across every sample: rebuilding a 100k entity world per
//! iteration would drown the thing being measured, and none of these libraries asks a user to.
//!
//! Benchmark ids read `query/<layout>/<n>_components/<library>/<entity count>`, so the criterion
//! report puts the three libraries on one chart per scenario. Throughput is set to the entity count,
//! which makes `elem/s` comparable across entity counts as well as across libraries.
//!
//! What each axis is asking:
//!
//! - **layout**: `1_archetype` is the friendly case, one contiguous run of entities. `5_archetypes`
//!   splits the same entities across five archetypes that a query has to fan out over, which is what
//!   any real world looks like after a few frames of adding components.
//! - **components**: 1, 2 and 3. Every extra component is another column to walk in lockstep, so the
//!   slope across these three says how the query machinery scales with arity rather than with size.
//! - **entity count**: 1k fits comfortably in cache, 100k does not.
//!
//! Passing order is deliberate: for each scenario the three libraries run back to back, so a machine
//! that heats up or drifts in clock speed over a long run drags all three the same way instead of
//! systematically penalising whichever one was registered last.

use std::hint::black_box;
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{BenchmarkGroup, BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use xynok_ecs_benches::config::warn_if_debug_build;
use xynok_ecs_benches::workload::{ArchetypeLayout, ENTITY_COUNTS, QueryWorkload, count_label};
use xynok_ecs_benches::{bevy, stdvec, xynok};

fn bench_one<W: QueryWorkload>(group: &mut BenchmarkGroup<'_, WallTime>, entity_count: usize, layout: ArchetypeLayout)
{
    let mut storage = W::setup(entity_count, layout);
    let mut query = W::prepare_query(&mut storage);

    group.bench_function(BenchmarkId::new(W::NAME, count_label(entity_count)), |b| {
        b.iter(|| W::run_query_once(black_box(&mut storage), black_box(&mut query)));
    });
}

/// One criterion group per (layout, arity), holding all three libraries at every entity count.
fn bench_arity<X, B, V>(c: &mut Criterion, layout: ArchetypeLayout)
where
    X: QueryWorkload,
    B: QueryWorkload,
    V: QueryWorkload,
{
    let mut group = c.benchmark_group(format!("query/{}/{}_components", layout.slug(), X::COMPONENT_COUNT));
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    for &entity_count in &ENTITY_COUNTS
    {
        group.throughput(Throughput::Elements(entity_count as u64));
        bench_one::<X>(&mut group, entity_count, layout);
        bench_one::<B>(&mut group, entity_count, layout);
        bench_one::<V>(&mut group, entity_count, layout);
    }

    group.finish();
}

fn query_benches(c: &mut Criterion)
{
    warn_if_debug_build();

    for layout in ArchetypeLayout::ALL
    {
        bench_arity::<xynok::Query1, bevy::Query1, stdvec::Query1>(c, layout);
        bench_arity::<xynok::Query2, bevy::Query2, stdvec::Query2>(c, layout);
        bench_arity::<xynok::Query3, bevy::Query3, stdvec::Query3>(c, layout);
    }
}

criterion_group!(query, query_benches);
criterion_main!(query);
