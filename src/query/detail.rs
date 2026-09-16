use std::marker::PhantomData;
use std::ops::{Deref, DerefMut};

use crate::apis::identifies::XynokEcsError;
use crate::apis::internal_traits::{QueryTicks, TQueryColumn, TQueryParam, TReadOnlyQueryParam};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::{TComponent, TEnableAble};
use crate::chunk::layout::ChunkLayout;
use crate::chunk::{read_bit, write_bit};
use crate::query::access_scope::AccessScope;
use crate::query::variant::MutItem;
use crate::query::variant::column_descriptor;

/// Hands out a component together with its per-row state, for now only the enable bit.
///
/// `Q` is `&T` or `&mut T`, and `T` has to be [`TEnableAble`]. Unlike `Enabled<Q>` it keeps every
/// row, enabled or not, so a system can look at the bit and flip it, e.g.
/// `for (hp, mut mesh) in Query<(&Hp, Detail<&mut Mesh>)>` then `mesh.set_enabled(hp.0 > 0)`.
///
/// Only `Detail<&mut T>` can change the bit. `Detail<&T>` registers a read, and the scheduler
/// happily runs two readers side by side, so letting it write would be a data race.
///
/// Flipping the bit does not count as a change: `Changed<T>` only sees writes to the value.
///
/// `Q` is unbounded on the struct for the same reason as the filters: it also gets instantiated
/// with [`TQueryParam::Shape`] markers.
pub struct Detail<Q>(PhantomData<fn() -> Q>);

impl<Q: TQueryColumn> TQueryParam for Detail<Q>
where
    Q::Component: TEnableAble,
{
    type QueryItem<'a> = DetailItem<'a, Q>;
    // the wrapped column's own state, plus the offset of its enable region
    type ArchState = (Q::ArchState, usize);
    type Fetch = (Q::Fetch, *mut u8);
    type Shape = Detail<Q::Shape>;

    fn access_scope(component_specs: &mut ComponentSpecs) -> Result<AccessScope, XynokEcsError>
    {
        Q::access_scope(component_specs)
    }

    #[inline]
    #[track_caller]
    fn arch_state(layout: &ChunkLayout) -> Self::ArchState
    {
        let enable_offset = column_descriptor::<Q::Component>(layout).state_offset.enable_offset.unwrap_or_else(|| {
            panic!(
                "component `{}` has no enable region for `Detail`",
                std::any::type_name::<<Q::Component as TComponent>::StorageType>()
            )
        });
        (Q::arch_state(layout), enable_offset)
    }

    #[inline]
    unsafe fn fetch_init(state: Self::ArchState, chunk_ptr: *mut u8) -> Self::Fetch
    {
        unsafe { (Q::fetch_init(state.0, chunk_ptr), chunk_ptr.add(state.1)) }
    }

    #[inline]
    unsafe fn accepts(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> bool
    {
        unsafe { Q::accepts(&fetch.0, row, ticks) }
    }

    #[inline]
    unsafe fn fetch<'a>(fetch: &Self::Fetch, row: usize, ticks: QueryTicks) -> DetailItem<'a, Q>
    {
        DetailItem {
            value:      unsafe { Q::fetch(&fetch.0, row, ticks) },
            enable_ptr: fetch.1,
            row:        row,
        }
    }
}

/// Note: `Detail<Q>` is only read-only when `Q` is
unsafe impl<Q: TQueryColumn + TReadOnlyQueryParam> TReadOnlyQueryParam for Detail<Q> where Q::Component: TEnableAble {}

/// One row of a [`Detail`] query: the component plus its enable bit.
///
/// Reads like the component itself through `Deref`, so `mesh.vertex_count` still works. A
/// `Detail<&mut T>` item also has `DerefMut`, which goes through the usual change stamping.
pub struct DetailItem<'a, Q: TQueryParam>
{
    value:      Q::QueryItem<'a>,
    /// Start of the enable region of this column in the current chunk
    enable_ptr: *mut u8,
    row:        usize,
}

impl<'a, Q: TQueryColumn> DetailItem<'a, Q>
{
    /// Is this component enabled on this row?
    #[inline]
    pub fn enabled(&self) -> bool
    {
        // SAFETY: `enable_ptr` is the enable region of a live chunk and `row` is a live row of it
        unsafe { read_bit(self.enable_ptr, self.row) }
    }
}

impl<'a, 'q, T: TComponent + 'static> DetailItem<'a, &'q T>
{
    #[inline]
    pub fn value(&self) -> &T
    {
        self.value
    }
}

impl<'a, 'q, T: TComponent + 'static> DetailItem<'a, &'q mut T>
{
    #[inline]
    pub fn value(&self) -> &T
    {
        &self.value
    }

    /// The item a plain `&mut T` query would have handed out (`Mut<T>` or `&mut T`), so writes
    /// through it are stamped exactly like they would be there
    #[inline]
    pub fn value_mut(&mut self) -> &mut MutItem<'a, T>
    {
        &mut self.value
    }

    /// Turns the component on or off for this row. Does not mark it as changed.
    #[inline]
    pub fn set_enabled(&mut self, value: bool)
    {
        // SAFETY: same region as `enabled`. No other thread touches this byte: the scheduler
        // keeps other systems off `T` because we hold a write, and `iter_batch` never cuts two
        // batches through one byte of the enable region.
        unsafe { write_bit(self.enable_ptr, self.row, value) }
    }
}

impl<'a, 'q, T: TComponent + 'static> Deref for DetailItem<'a, &'q T>
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T
    {
        self.value
    }
}

impl<'a, 'q, T: TComponent + 'static> Deref for DetailItem<'a, &'q mut T>
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &T
    {
        &self.value
    }
}

impl<'a, 'q, T: TComponent + 'static> DerefMut for DetailItem<'a, &'q mut T>
{
    #[inline]
    fn deref_mut(&mut self) -> &mut T
    {
        &mut self.value
    }
}
