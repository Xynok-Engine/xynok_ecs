use crate::apis::constants::{ChangedTick, BITS_PER_BYTE};
use crate::apis::identifies::{StateDetection, XynokEcsError};
use crate::apis::params::{ChunkTakeComponentParams, SwappedRow};
use crate::apis::traits::TComponent;
use crate::chunk::column::{ColumnDescriptor, StateOffset};
use crate::chunk::layout::ChunkLayout;
use crate::chunk::migration::{ColumnInSrc, ComponentMigration};
use crate::entity::Entity;

pub(crate) mod layout;
pub(crate) mod column;
pub(crate) mod migration;

mod header;

// Free functions operating on an already-resolved region pointer (chunk base + state offset).
// The query side (`TQueryParam::fetch_init`) resolves that
// pointer once per chunk (like `current_col_ptr`) and call these directly per row, with no
// `Chunk` borrow and no `component_col_descriptors` lookup in the hot loop.
#[inline]
pub(crate) unsafe fn read_bit(region_ptr: *const u8, row: usize) -> bool
{
    unsafe {
        let byte = *region_ptr.add(row / BITS_PER_BYTE);
        (byte >> (row % BITS_PER_BYTE)) & 1 == 1
    }
}
#[inline]
pub(crate) unsafe fn write_bit(region_ptr: *mut u8, row: usize, value: bool)
{
    unsafe {
        let byte_ptr = region_ptr.add(row / BITS_PER_BYTE);
        let mask = 1u8 << (row % BITS_PER_BYTE);
        if value
        {
            *byte_ptr |= mask;
        }
        else
        {
            *byte_ptr &= !mask;
        }
    }
}
#[inline]
pub(crate) unsafe fn read_changed_tick(region_ptr: *const u8, row: usize) -> ChangedTick
{
    unsafe { *(region_ptr as *const ChangedTick).add(row) }
}
#[inline]
pub(crate) unsafe fn write_changed_tick(region_ptr: *mut u8, row: usize, tick: ChangedTick)
{
    unsafe { *(region_ptr as *mut ChangedTick).add(row) = tick };
}

/// Moves one row's state (enable bit, added tick, changed tick) from `from` to `to` inside the
/// same chunk.
///
/// A row's state lives in its own region, not next to the component value, so the byte copy
/// that moves the last row into a freed slot leaves the state behind. Without this the row that
/// got swapped down inherits the state of whoever used to sit there: a disabled component comes
/// back enabled, an old `changed` tick suddenly looks fresh.
///
/// The source row is left untouched. It is either about to be dropped (`swap_remove_at`) or
/// about to be overwritten by a fresh `write_at`, which re-seeds every state it owns.
#[inline]
pub(crate) unsafe fn move_state_within(chunk_ptr: *mut u8, state: &StateOffset, from: usize, to: usize)
{
    unsafe {
        if let Some(offset) = state.enable_offset
        {
            let region = chunk_ptr.add(offset);
            write_bit(region, to, read_bit(region, from));
        }
        if let Some(offset) = state.added_offset
        {
            let region = chunk_ptr.add(offset);
            write_changed_tick(region, to, read_changed_tick(region, from));
        }
        if let Some(offset) = state.changed_offset
        {
            let region = chunk_ptr.add(offset);
            write_changed_tick(region, to, read_changed_tick(region, from));
        }
    }
}

/// Moves the chunk's last row into the hole a row just left behind, value and state together.
///
/// Every migration case ends here: whether the component travelled with the entity, was dropped
/// in place or simply abandoned, the slot it used to sit in still has to be filled so the column
/// stays dense.
#[inline]
pub(crate) unsafe fn backfill_last_row(chunk_ptr: *mut u8, src: &ColumnInSrc, last: usize, to: usize)
{
    unsafe {
        let target_slot = chunk_ptr.add(src.offset).add(to * src.byte_size);
        let last_val = chunk_ptr.add(src.offset).add(last * src.byte_size);
        std::ptr::copy_nonoverlapping(last_val, target_slot, src.byte_size);
        move_state_within(chunk_ptr, &src.state_offset, last, to);
    }
}

/// Copies one row's state from a chunk into another chunk's row, used when an entity migrates
/// to a different archetype and takes its components along.
///
/// Only the state kinds *both* layouts track are copied. The two layouts describe the same
/// component, so in practice they agree, but a state the destination does not carry simply has
/// nowhere to go.
#[inline]
pub(crate) unsafe fn copy_state_across(src_ptr: *const u8, src_state: &StateOffset, src_row: usize, dst_ptr: *mut u8, dst_state: &StateOffset, dst_row: usize)
{
    unsafe {
        if let (Some(src_offset), Some(dst_offset)) = (src_state.enable_offset, dst_state.enable_offset)
        {
            write_bit(dst_ptr.add(dst_offset), dst_row, read_bit(src_ptr.add(src_offset), src_row));
        }
        if let (Some(src_offset), Some(dst_offset)) = (src_state.added_offset, dst_state.added_offset)
        {
            write_changed_tick(dst_ptr.add(dst_offset), dst_row, read_changed_tick(src_ptr.add(src_offset), src_row));
        }
        if let (Some(src_offset), Some(dst_offset)) = (src_state.changed_offset, dst_state.changed_offset)
        {
            write_changed_tick(dst_ptr.add(dst_offset), dst_row, read_changed_tick(src_ptr.add(src_offset), src_row));
        }
    }
}

pub struct Chunk
{
    ptr:     *mut u8,
    len:     usize,
    max_len: usize,
}

impl Chunk
{
    pub fn new(layout: &ChunkLayout) -> Self
    {
        let ptr = unsafe { std::alloc::alloc(layout.alloc_layout) };
        // Panic if we fail to allocate additional memory
        if ptr.is_null()
        {
            std::alloc::handle_alloc_error(layout.alloc_layout);
        }
        unsafe {
            std::ptr::write_bytes(ptr, 0u8, layout.header.size);
        }
        Self {
            ptr:     ptr,
            len:     0,
            max_len: layout.max_len,
        }
    }

    #[inline]
    pub fn ptr(&self) -> *mut u8
    {
        self.ptr
    }
    #[inline]
    pub fn len(&self) -> usize
    {
        self.len
    }

    #[inline]
    pub fn is_full(&self) -> bool
    {
        self.len() >= self.max_len
    }
    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.len < 1
    }

    #[inline]
    pub fn get_component<C: TComponent + 'static>(&self, layout: &ChunkLayout, row: usize) -> Result<&C, XynokEcsError>
    {
        if row >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(row, self.len()));
        }
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { &*(base as *const C).add(row) })
    }
    #[inline]
    pub fn get_component_mut<C: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize) -> Result<&mut C, XynokEcsError>
    {
        if row >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(row, self.len()));
        }
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { &mut *(base as *mut C).add(row) })
    }

    /// A slice `&[C]` of the component column `C` (inline) within the chunk, containing all rows
    #[inline]
    pub fn get_components<C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<&[C], XynokEcsError>
    {
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { std::slice::from_raw_parts(base as *const C, self.len()) })
    }

    #[inline]
    pub fn get_components_mut<C: TComponent + 'static>(&mut self, layout: &ChunkLayout) -> Result<&mut [C], XynokEcsError>
    {
        let base = self.components_ptr::<C>(layout)?;
        Ok(unsafe { std::slice::from_raw_parts_mut(base as *mut C, self.len()) })
    }

    #[inline]
    pub fn get_entity(&self, layout: &ChunkLayout, row: usize) -> Result<&Entity, XynokEcsError>
    {
        if row >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(row, self.len()));
        }
        Ok(unsafe { self.get_entity_uncheck(layout, row) })
    }
    #[inline]
    pub fn get_entities(&self, layout: &ChunkLayout) -> Result<&[Entity], XynokEcsError>
    {
        unsafe {
            let entities_ptr = self.ptr.add(layout.header.entities_offset);
            Ok(std::slice::from_raw_parts(entities_ptr as *const Entity, self.len()))
        }
    }
    pub fn get_entities_components<C: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<(&[Entity], &[C]), XynokEcsError>
    {
        let entities = self.get_entities(layout)?;
        let components = self.get_components::<C>(layout)?;
        Ok((entities, components))
    }

    pub fn get_entities_components_mut<C: TComponent + 'static>(&mut self, layout: &ChunkLayout) -> Result<(&[Entity], &mut [C]), XynokEcsError>
    {
        // Cột entity và cột component nằm ở hai vùng nhớ tách biệt trong chunk, nên mượn
        // đồng thời một &[Entity] và một &mut [C] là an toàn. Borrow checker không tự thấy
        // điều đó, nên ở đây đi qua raw pointer rồi dựng lại slice với lifetime của &mut self.
        let len = self.len();
        let entities_ptr = unsafe { self.ptr.add(layout.header.entities_offset) } as *const Entity;
        let components_ptr = self.components_ptr::<C>(layout)? as *mut C;
        unsafe {
            Ok((
                std::slice::from_raw_parts(entities_ptr, len),
                std::slice::from_raw_parts_mut(components_ptr, len),
            ))
        }
    }
}

impl Chunk
{
    /// Fetch data from another chunk and incorporate it into the current one
    /// Returns the swapped indices of an entity in the chunk that was taken and subsequently swapped
    pub(crate) unsafe fn take_from(&mut self, params: ChunkTakeComponentParams) -> Result<Option<SwappedRow>, XynokEcsError>
    {
        let last = params.src_chunk.len() - 1;
        let is_last = params.from == last;
        let src_ptr = params.src_chunk.ptr();
        let dst_ptr = self.ptr();
        unsafe {
            // All decisions regarding whether a component should move, drop in place, or be left behind were finalized during the planning phase.
            // At this stage, we simply iterate over the slice. No hashing is required.
            for mig in params.migration.components.iter()
            {
                match mig
                {
                    ComponentMigration::Move(m) =>
                    {
                        let src_slot = src_ptr.add(m.src.offset).add(params.from * m.src.byte_size);
                        let dst_slot = dst_ptr.add(m.dst_offset).add(params.to * m.src.byte_size);
                        std::ptr::copy_nonoverlapping(src_slot, dst_slot, m.src.byte_size);
                        // the component travels with the entity, so its state has to travel too: a
                        // disabled component must stay disabled after an `add_component`, and its
                        // added/changed ticks must keep pointing at the run that actually touched it
                        copy_state_across(src_ptr, &m.src.state_offset, params.from, dst_ptr, &m.dst_state_offset, params.to);
                    }
                    // when removing a component, the dst often won't have all the components from the src
                    ComponentMigration::Abandon(_) =>
                    {}
                }

                if !is_last
                {
                    backfill_last_row(src_ptr, mig.src(), last, params.from);
                }
            }
            let moved_e = *params.src_chunk.get_entity_uncheck(params.src_layout, params.from);
            let new_src_e = match is_last
            {
                true => Entity::NULL,
                false => *params.src_chunk.get_entity_uncheck(params.src_layout, last),
            };
            *self.get_entity_uncheck_mut(params.dst_layout, params.to) = moved_e;
            *params.src_chunk.get_entity_uncheck_mut(params.src_layout, params.from) = new_src_e;
        }

        if is_last
        {
            return Ok(None);
        }

        let swapped = unsafe {
            SwappedRow {
                // get the entity from the `src.from` because we already swapped it
                e:    *params.src_chunk.get_entity_uncheck(params.src_layout, params.from),
                from: last,
                to:   params.from,
            }
        };
        Ok(Some(swapped))
    }
    pub(crate) unsafe fn swap_remove_at(&mut self, layout: &ChunkLayout, idx: usize) -> Result<Option<SwappedRow>, XynokEcsError>
    {
        if idx >= self.len()
        {
            return Err(XynokEcsError::IdxIsOutOfChunkLen(idx, self.len()));
        }

        let last = self.len - 1;
        let is_last = idx == last;
        let chunk_ptr = self.ptr;
        unsafe {
            for col in layout.columns.iter()
            {
                let item_size = col.byte_size;
                let target_slot = chunk_ptr.add(col.offset).add(idx * item_size);
                (col.fn_drop)(target_slot);

                if !is_last
                {
                    let last_val = chunk_ptr.add(col.offset).add(last * item_size);
                    std::ptr::copy_nonoverlapping(last_val, target_slot, item_size);
                    move_state_within(chunk_ptr, &col.state_offset, last, idx);
                }
            }
            let new_src_e = match is_last
            {
                true => Entity::NULL,
                false => *self.get_entity_uncheck(layout, last),
            };
            *self.get_entity_uncheck_mut(layout, idx) = new_src_e;
        }

        if is_last
        {
            return Ok(None);
        }

        Ok(Some(unsafe {
            SwappedRow {
                e:    *self.get_entity_uncheck(layout, last),
                from: last,
                to:   idx,
            }
        }))
    }
}
impl Chunk
{
    #[inline]
    pub(crate) unsafe fn get_entity_uncheck(&self, layout: &ChunkLayout, row: usize) -> &Entity
    {
        unsafe {
            let entities_ptr = self.ptr.add(layout.header.entities_offset);
            &*(entities_ptr as *const Entity).add(row)
        }
    }
    #[inline]
    pub(crate) unsafe fn get_entity_uncheck_mut(&mut self, layout: &ChunkLayout, row: usize) -> &mut Entity
    {
        unsafe {
            let entities_ptr = self.ptr.add(layout.header.entities_offset);
            &mut *(entities_ptr as *mut Entity).add(row)
        }
    }
}
impl Chunk
{
    #[inline]
    pub(crate) unsafe fn increase_len(&mut self)
    {
        self.len += 1;
    }
    #[inline]
    pub(crate) unsafe fn decrease_len(&mut self)
    {
        self.len -= 1;
    }
    /// Drop the old value and assign the new one. The component already existed in this row, so
    /// only `changed` moves; `enable`/`added` are untouched (see `write_at` for a fresh insert)
    pub(crate) unsafe fn replace_at<T: TComponent + 'static>(
        &mut self,
        layout: &ChunkLayout,
        row: usize,
        value: T,
        tick: ChangedTick,
    ) -> Result<(), XynokEcsError>
    {
        let col_ptr = self.components_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            *slot = value;
        }
        if matches!(T::STATE_DETECTION, StateDetection::ChangeAble | StateDetection::EnableAbleAndChangeAble)
        {
            self.set_changed_tick::<T>(layout, row, tick)?;
        }
        Ok(())
    }
    /// Used after a move: if the source row already had `T`, its value and state were moved into
    /// `row`, so this is an overwrite (`replace_at`). Otherwise `T` is new here (`write_at`).
    pub(crate) unsafe fn write_or_replace_at<T: TComponent + 'static>(
        &mut self,
        src_layout: &ChunkLayout,
        dst_layout: &ChunkLayout,
        row: usize,
        value: T,
        tick: ChangedTick,
    ) -> Result<(), XynokEcsError>
    {
        match src_layout.component_col_descriptors.contains_key(&std::any::TypeId::of::<T::StorageType>())
        {
            true => unsafe { self.replace_at::<T>(dst_layout, row, value, tick) },
            false => unsafe { self.write_at::<T>(dst_layout, row, value, tick) },
        }
    }
    /// Writes directly to memory without dropping the old value. Typically used when the memory
    /// has just been initialized, i.e. this component is appearing in this row for the first
    /// time: `enable` is seeded to `T::ENABLE_VALUE` and `added`/`changed` both start at `tick`
    pub(crate) unsafe fn write_at<T: TComponent + 'static>(
        &mut self,
        layout: &ChunkLayout,
        row: usize,
        value: T,
        tick: ChangedTick,
    ) -> Result<(), XynokEcsError>
    {
        let col_ptr = self.components_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            slot.write(value);
        }
        if matches!(T::STATE_DETECTION, StateDetection::EnableAble | StateDetection::EnableAbleAndChangeAble)
        {
            self.set_enable_bit::<T>(layout, row, T::ENABLE_VALUE)?;
        }
        if matches!(T::STATE_DETECTION, StateDetection::ChangeAble | StateDetection::EnableAbleAndChangeAble)
        {
            self.set_added_tick::<T>(layout, row, tick)?;
            self.set_changed_tick::<T>(layout, row, tick)?;
        }
        Ok(())
    }
    pub(crate) unsafe fn take_at<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize) -> Result<T, XynokEcsError>
    {
        let col_ptr = self.components_ptr::<T>(layout)?;
        unsafe {
            let slot = (col_ptr as *mut T).add(row);
            Ok(slot.read())
        }
    }

    pub(crate) fn dispose(&mut self, layout: &ChunkLayout)
    {
        for col in layout.columns.iter()
        {
            let mut counter = 0usize;
            while counter < self.len()
            {
                let slot = unsafe { self.ptr().add(col.offset).add(counter * col.byte_size) };
                (col.fn_drop)(slot);
                counter += 1;
            }
        }

        unsafe {
            std::alloc::dealloc(self.ptr, layout.alloc_layout);
        }
        self.len = 0;
    }
}
impl Chunk
{
    #[inline]
    fn components_ptr<T: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<*mut u8, XynokEcsError>
    {
        let col_des = match layout.component_col_descriptors.get(&std::any::TypeId::of::<T::StorageType>())
        {
            Some(des) => des,
            None =>
            {
                return Err(XynokEcsError::ChunkDoesNotContainComponent(
                    std::any::type_name::<T::QueryType>(),
                    std::any::type_name::<T::StorageType>(),
                ));
            }
        };

        Ok(unsafe { self.ptr.add(col_des.offset) })
    }

    /// Answers "would `write_at::<T>` find every column it needs in `layout`?" by doing exactly the
    /// lookups `write_at` does, without touching a single byte of memory. Every way `write_at` can
    /// fail depends only on `T` and the layout, never on the row or the chunk's contents, so an `Ok`
    /// here means the matching `write_at` cannot fail.
    ///
    /// This is what lets `take_and_write_from` move a row only after it knows the write that follows
    /// will go through: a write failing halfway used to leave the source row moved out with neither
    /// chunk's `len` updated (issue #29).
    pub(crate) fn validate_writable<T: TComponent + 'static>(layout: &ChunkLayout) -> Result<(), XynokEcsError>
    {
        let col_des = Self::validate_column_present::<T>(layout)?;

        if matches!(T::STATE_DETECTION, StateDetection::EnableAble | StateDetection::EnableAbleAndChangeAble)
        {
            col_des
                .state_offset
                .enable_offset
                .ok_or(XynokEcsError::ComponentStateNotAvailable(std::any::type_name::<T::StorageType>(), "enable"))?;
        }
        if matches!(T::STATE_DETECTION, StateDetection::ChangeAble | StateDetection::EnableAbleAndChangeAble)
        {
            col_des
                .state_offset
                .added_offset
                .ok_or(XynokEcsError::ComponentStateNotAvailable(std::any::type_name::<T::StorageType>(), "added"))?;
            col_des
                .state_offset
                .changed_offset
                .ok_or(XynokEcsError::ComponentStateNotAvailable(std::any::type_name::<T::StorageType>(), "changed"))?;
        }
        Ok(())
    }

    /// The same idea as [`Chunk::validate_writable`], for the paths that only read a component out
    /// of a row (`take_at`): those touch the column itself and none of its state regions.
    pub(crate) fn validate_takeable<T: TComponent + 'static>(layout: &ChunkLayout) -> Result<(), XynokEcsError>
    {
        Self::validate_column_present::<T>(layout)?;
        Ok(())
    }

    fn validate_column_present<T: TComponent + 'static>(layout: &ChunkLayout) -> Result<&ColumnDescriptor, XynokEcsError>
    {
        match layout.component_col_descriptors.get(&std::any::TypeId::of::<T::StorageType>())
        {
            Some(des) => Ok(des),
            None => Err(XynokEcsError::ChunkDoesNotContainComponent(
                std::any::type_name::<T::QueryType>(),
                std::any::type_name::<T::StorageType>(),
            )),
        }
    }

    #[inline]
    fn state_offset<T: TComponent + 'static>(&self, layout: &ChunkLayout) -> Result<StateOffset, XynokEcsError>
    {
        let col_des = match layout.component_col_descriptors.get(&std::any::TypeId::of::<T::StorageType>())
        {
            Some(des) => des,
            None =>
            {
                return Err(XynokEcsError::ChunkDoesNotContainComponent(
                    std::any::type_name::<T::QueryType>(),
                    std::any::type_name::<T::StorageType>(),
                ));
            }
        };
        Ok(col_des.state_offset.clone())
    }

    // ---- fast path: caller already resolved & cached `region_offset` once per chunk
    // (mirrors how a query's `fetch_init` caches the column pointer once per chunk instead of
    // hitting `component_col_descriptors` on every row). No HashMap lookup, no Result.
    #[inline]
    pub(crate) unsafe fn set_bit_unchecked(&mut self, region_offset: usize, row: usize, value: bool)
    {
        unsafe { write_bit(self.ptr.add(region_offset), row, value) }
    }
    #[inline]
    pub(crate) unsafe fn set_changed_tick_unchecked(&mut self, region_offset: usize, row: usize, tick: ChangedTick)
    {
        unsafe { write_changed_tick(self.ptr.add(region_offset), row, tick) }
    }

    // ---- checked path: does the `component_col_descriptors` lookup every call, meant for
    // one-off writes outside a query's hot loop. All of them delegate to the `_unchecked` fns
    // above. There is no matching read here: queries read state through the `read_bit` /
    // `read_changed_tick` free fns, which take an already-resolved region pointer.
    #[inline]
    pub(crate) fn set_enable_bit<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, value: bool) -> Result<(), XynokEcsError>
    {
        let offset = self
            .state_offset::<T>(layout)?
            .enable_offset
            .ok_or(XynokEcsError::ComponentStateNotAvailable(std::any::type_name::<T::StorageType>(), "enable"))?;
        unsafe { self.set_bit_unchecked(offset, row, value) };
        Ok(())
    }

    #[inline]
    pub(crate) fn set_added_tick<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, tick: ChangedTick) -> Result<(), XynokEcsError>
    {
        let offset = self
            .state_offset::<T>(layout)?
            .added_offset
            .ok_or(XynokEcsError::ComponentStateNotAvailable(std::any::type_name::<T::StorageType>(), "added"))?;
        unsafe { self.set_changed_tick_unchecked(offset, row, tick) };
        Ok(())
    }

    #[inline]
    pub(crate) fn set_changed_tick<T: TComponent + 'static>(&mut self, layout: &ChunkLayout, row: usize, tick: ChangedTick) -> Result<(), XynokEcsError>
    {
        let offset = self
            .state_offset::<T>(layout)?
            .changed_offset
            .ok_or(XynokEcsError::ComponentStateNotAvailable(std::any::type_name::<T::StorageType>(), "changed"))?;
        unsafe { self.set_changed_tick_unchecked(offset, row, tick) };
        Ok(())
    }
}
