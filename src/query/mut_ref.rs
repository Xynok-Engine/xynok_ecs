use std::fmt;
use std::ops::{Deref, DerefMut};

use crate::apis::constants::ChangedTick;
use crate::chunk::write_changed_tick;

/// Everything a query item needs on top of its own column pointer.
///
/// Resolved once per chunk by the `SrcAccess` that walks it, the same way the column pointer is,
/// so handing out a row stays a couple of pointer adds with no descriptor lookup in the loop.
#[derive(Clone, Copy)]
pub struct WriteStamp
{
    /// Start of this column's `changed` region in the current chunk, or null when the component
    /// does not track changes at all. A `Mut` only ever exists for a `ChangeAble` component, and
    /// every chunk holding such a column carries its `changed` region (see `ChunkHeader::new`),
    /// so by the time a stamp reaches a `Mut` this pointer is non-null. The null case is just the
    /// resting value carried by read-only items and by an access struct that has not reached its
    /// first chunk yet.
    pub changed_ptr: *mut u8,
    /// The tick a write should be stamped with, i.e. the world's tick as of this query
    pub tick:        ChangedTick,
}

impl WriteStamp
{
    /// A stamp that records nothing, for read-only items and for the initial state of an
    /// access struct that has not reached its first chunk yet
    pub const NONE: Self = Self {
        changed_ptr: std::ptr::null_mut(),
        tick:        0,
    };
}

/// What a `&mut T` query hands out for one row, when `T` is a `ChangeAble` component. A
/// component without change detection skips this entirely and gets the `&mut T` itself, see
/// [`TMutPolicy`].
///
/// It behaves like `&mut T` through `Deref`/`DerefMut`, so `hp.0 = 5` and `let v = hp.0` read
/// exactly as they did before. The difference is that only the `DerefMut` side stamps the row's
/// changed tick, so merely iterating a `Query<&mut Hp>` does not tell every later
/// `Changed<Hp>` that all of those rows were written.
///
/// Needing a plain `&mut T` back (to pass along to a function, say) is just `&mut *value`.
pub struct Mut<'a, T>
{
    value: &'a mut T,
    stamp: WriteStamp,
    row:   usize,
}

impl<'a, T> Mut<'a, T>
{
    /// # Safety
    /// `stamp.changed_ptr` must point at the start of the `changed` region of the very column
    /// `value` was read out of, in a chunk where `row` is a live row. Null is not allowed:
    /// writing through the result dereferences it unconditionally.
    #[inline]
    pub(crate) unsafe fn new(value: &'a mut T, stamp: WriteStamp, row: usize) -> Self
    {
        Self {
            value: value,
            stamp: stamp,
            row:   row,
        }
    }

    /// Writes without stamping the changed tick. For the rare case where a system rewrites a
    /// value it knows is identical and does not want to wake every `Changed` reader.
    #[inline]
    pub fn bypass_change_detection(this: &mut Self) -> &mut T
    {
        this.value
    }

    /// Stamps the row as changed without going through a write, the counterpart of
    /// [`Self::bypass_change_detection`]
    #[inline]
    pub fn mark_changed(this: &mut Self)
    {
        // No null check: `Mut` is only handed out for a `ChangeAble` component, and such a
        // column always has its `changed` region in the chunk. See `Mut::new`'s safety contract.
        debug_assert!(!this.stamp.changed_ptr.is_null(), "a `Mut` was built on a column with no `changed` region");
        unsafe { write_changed_tick(this.stamp.changed_ptr, this.row, this.stamp.tick) };
    }
}

impl<'a, T> Deref for Mut<'a, T>
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T
    {
        self.value
    }
}

impl<'a, T> DerefMut for Mut<'a, T>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut T
    {
        Self::mark_changed(self);
        self.value
    }
}

impl<'a, T> AsRef<T> for Mut<'a, T>
{
    #[inline]
    fn as_ref(&self) -> &T
    {
        self.value
    }
}

impl<'a, T> AsMut<T> for Mut<'a, T>
{
    #[inline]
    fn as_mut(&mut self) -> &mut T
    {
        self.deref_mut()
    }
}

// Debug/PartialEq forwarded so `Mut<T>` keeps reading like the `&mut T` it replaced in
// assertions and `println!`
impl<'a, T: fmt::Debug> fmt::Debug for Mut<'a, T>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result
    {
        self.value.fmt(f)
    }
}

impl<'a, T: PartialEq> PartialEq<T> for Mut<'a, T>
{
    fn eq(&self, other: &T) -> bool
    {
        self.value == other
    }
}

/// How a `&mut T` query hands out one row, picked by the component itself through
/// [`TComponent::MutPolicy`](crate::apis::traits::TComponent::MutPolicy).
///
/// The point is that a component which never asked for change detection should not pay for it.
/// A `ChangeAble` component goes through [`TrackChanges`] and gets a [`Mut<T>`]; everything else
/// goes through [`NoTracking`] and gets a plain `&mut T`, with the stamp thrown away at compile
/// time instead of checked at runtime.
pub trait TMutPolicy
{
    /// What one row looks like to the caller
    type Item<'a, T: 'a>;

    /// # Safety
    /// `ptr` must point at a live `T` at `row` of the current chunk, and `stamp` must satisfy
    /// [`Mut::new`]'s contract whenever this policy actually builds a [`Mut`].
    unsafe fn make<'a, T: 'a>(ptr: *mut T, stamp: WriteStamp, row: usize) -> Self::Item<'a, T>;
}

/// Policy for a `ChangeAble` component: hand out a [`Mut`], which stamps the row on write
pub struct TrackChanges;

/// Policy for everything else: hand out the `&mut T` the caller asked for, nothing wrapped
/// around it
pub struct NoTracking;

impl TMutPolicy for TrackChanges
{
    type Item<'a, T: 'a> = Mut<'a, T>;

    #[inline]
    unsafe fn make<'a, T: 'a>(ptr: *mut T, stamp: WriteStamp, row: usize) -> Mut<'a, T>
    {
        unsafe { Mut::new(&mut *ptr, stamp, row) }
    }
}

impl TMutPolicy for NoTracking
{
    type Item<'a, T: 'a> = &'a mut T;

    #[inline]
    unsafe fn make<'a, T: 'a>(ptr: *mut T, _stamp: WriteStamp, _row: usize) -> &'a mut T
    {
        unsafe { &mut *ptr }
    }
}
