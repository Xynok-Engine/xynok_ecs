use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::apis::constants::CHUNK_SIZE_IN_BYTE;

fn value(component_id: usize) -> SharedValue
{
    SharedValue {
        component_id: component_id,
        key_id:       0,
    }
}

// The storage is always handed the key, it never calls `shared_key` itself. The keys below only
// need to exist.

#[derive(Clone)]
struct Small(u16);
impl TSharedComponent for Small
{
    type Key = u8;
    fn shared_key(&self) -> u8
    {
        self.0 as u8
    }
}

const BIG_LEN: usize = 32 * 1024;
#[derive(Clone)]
#[repr(align(128))]
struct Huge([u8; BIG_LEN]);
impl TSharedComponent for Huge
{
    type Key = u64;
    fn shared_key(&self) -> u64
    {
        self.0[0] as u64
    }
}

#[derive(Clone)]
struct Unit;
impl TSharedComponent for Unit
{
    type Key = ();
    fn shared_key(&self) {}
}

#[test]
fn entries_share_one_aligned_allocation_that_can_outgrow_a_chunk()
{
    let small = ArchetypeChunk::rebuild::<Small>(&ArchetypeChunk::default(), Some((value(3), 7, Small(100))));
    let huge = ArchetypeChunk::rebuild::<Huge>(&small, Some((value(1), 8, Huge([9; BIG_LEN]))));
    let all = ArchetypeChunk::rebuild::<Unit>(&huge, Some((value(2), (), Unit)));

    assert!(all.alloc_layout.size() > CHUNK_SIZE_IN_BYTE);
    assert_eq!(all.values().map(|v| v.component_id).collect::<Vec<_>>(), [1, 2, 3], "entries follow component id order");
    assert_eq!(all.component_bit_set().iter().collect::<Vec<_>>(), [1, 2, 3]);

    let mut spans = Vec::new();
    for column in &all.columns
    {
        assert_eq!(column.offset % column.descriptor.entry_layout.align(), 0);
        spans.push((column.offset, column.offset + column.descriptor.entry_layout.size()));
    }
    assert!(spans.windows(2).all(|pair| pair[0].1 <= pair[1].0), "entries must not overlap");

    let (key, huge_value) = all.entry_of::<Huge>().unwrap();
    assert_eq!(huge_value as usize % 128, 0);
    unsafe {
        assert_eq!(*key, 8);
        assert_eq!((*huge_value).0[BIG_LEN - 1], 9);
    }
    assert_eq!(all.key::<Small>(), Some(&7));
    assert!(all.contains_value(value(2)));
    assert!(!all.contains_value(SharedValue { component_id: 2, key_id: 1 }));
    assert_eq!(small.key::<Huge>(), None, "building a new storage leaves the source alone");
}

/// Counts live values: +1 when made or cloned, -1 when dropped
struct Counted(Arc<AtomicUsize>);
impl Counted
{
    fn new(live: &Arc<AtomicUsize>) -> Self
    {
        live.fetch_add(1, Ordering::SeqCst);
        Self(live.clone())
    }
}
impl Clone for Counted
{
    fn clone(&self) -> Self
    {
        Self::new(&self.0)
    }
}
impl Drop for Counted
{
    fn drop(&mut self)
    {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone)]
struct A(#[allow(dead_code)] Counted);
impl TSharedComponent for A
{
    type Key = usize;
    fn shared_key(&self) -> usize
    {
        0
    }
}
#[derive(Clone)]
struct B(#[allow(dead_code)] Counted);
impl TSharedComponent for B
{
    type Key = usize;
    fn shared_key(&self) -> usize
    {
        0
    }
}

#[test]
fn rebuild_clones_kept_values_and_moves_the_inserted_one()
{
    let live = Arc::new(AtomicUsize::new(0));
    let count = || live.load(Ordering::SeqCst);

    let a = ArchetypeChunk::rebuild::<A>(&ArchetypeChunk::default(), Some((value(0), 1, A(Counted::new(&live)))));
    assert_eq!(count(), 1);

    let ab = ArchetypeChunk::rebuild::<B>(&a, Some((value(1), 2, B(Counted::new(&live)))));
    assert_eq!(count(), 3, "A is cloned, B is moved in");

    let b = ArchetypeChunk::rebuild::<A>(&ab, None);
    assert_eq!(count(), 4, "only B is cloned");
    assert_eq!(b.key::<A>(), None);
    assert_eq!(b.key::<B>(), Some(&2));

    let replaced = ArchetypeChunk::rebuild::<A>(&ab, Some((SharedValue { component_id: 0, key_id: 5 }, 5, A(Counted::new(&live)))));
    assert_eq!(count(), 6, "the old A is not cloned when it gets replaced");
    assert_eq!(replaced.key::<A>(), Some(&5));

    let copy = replaced.clone();
    assert_eq!(count(), 8);

    drop((a, ab, b, replaced, copy));
    assert_eq!(count(), 0);
}

/// Panics when cloned. The inner value is never read, it is only there to be counted.
struct Fragile(#[allow(dead_code)] Counted);
impl Clone for Fragile
{
    fn clone(&self) -> Self
    {
        panic!("clone failure");
    }
}
#[derive(Clone)]
struct X(#[allow(dead_code)] Counted);
impl TSharedComponent for X
{
    type Key = ();
    fn shared_key(&self) {}
}
#[derive(Clone)]
struct Y(#[allow(dead_code)] Fragile);
impl TSharedComponent for Y
{
    type Key = ();
    fn shared_key(&self) {}
}
#[derive(Clone)]
struct Z(#[allow(dead_code)] Counted);
impl TSharedComponent for Z
{
    type Key = ();
    fn shared_key(&self) {}
}

#[test]
fn a_panicking_clone_leaks_nothing()
{
    let live = Arc::new(AtomicUsize::new(0));
    let x = ArchetypeChunk::rebuild::<X>(&ArchetypeChunk::default(), Some((value(0), (), X(Counted::new(&live)))));
    let xy = ArchetypeChunk::rebuild::<Y>(&x, Some((value(1), (), Y(Fragile(Counted::new(&live))))));
    drop(x);
    assert_eq!(live.load(Ordering::SeqCst), 2);

    // X gets cloned first, then Y panics before Z is written
    let failed = catch_unwind(AssertUnwindSafe(|| ArchetypeChunk::rebuild::<Z>(&xy, Some((value(2), (), Z(Counted::new(&live)))))));
    assert!(failed.is_err());
    assert_eq!(live.load(Ordering::SeqCst), 2, "the cloned X and the unwritten Z must both be dropped");

    drop(xy);
    assert_eq!(live.load(Ordering::SeqCst), 0);
}
