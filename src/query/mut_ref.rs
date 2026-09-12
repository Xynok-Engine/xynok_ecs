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
    /// does not track changes at all (not `ChangeAble`, or the archetype predates the column).
    /// A null here simply means a write has nowhere to record itself.
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

/// What a `&mut T` query hands out for one row.
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
    /// `stamp.changed_ptr` must either be null or point at the start of the `changed` region of
    /// the very column `value` was read out of, in a chunk where `row` is a live row.
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
        if !this.stamp.changed_ptr.is_null()
        {
            unsafe { write_changed_tick(this.stamp.changed_ptr, this.row, this.stamp.tick) };
        }
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
