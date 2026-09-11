//! Storage for the shared components of one archetype.
//!
//! Every entity of an archetype uses the same shared values, so an archetype keeps exactly one
//! entry per shared component type. All entries sit next to each other in one aligned
//! allocation, the same way a `Chunk` packs its columns. Unlike a chunk, the size is not capped:
//! it is whatever the entries need.
//!
//! The set of shared components and their keys never changes once the storage is built. A value
//! can be overwritten in place, but an entity that needs another key moves to another archetype,
//! and that archetype gets storage of its own from [`ArchetypeChunk::rebuild`].

use std::alloc::{Layout, alloc, dealloc, handle_alloc_error};
use std::any::TypeId;
use std::ptr::NonNull;

use crate::apis::ComponentDescriptor;
use crate::apis::identifies::StorageLocation;
use crate::collection::component_bit_set::ComponentBitSet;
use crate::shared::TSharedComponent;

#[cfg(test)]
mod tests;

/// One stored shared value. `key` is the key the archetype was created with, kept next to the
/// value so a key check never has to touch a world-wide table. It is not updated when the value
/// is overwritten.
struct Entry<C: TSharedComponent>
{
    key:   C::Key,
    value: C,
}

/// Identity of one shared value inside a world: which shared type, and which of its interned keys.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub struct SharedValue
{
    pub component_id: usize,
    pub key_id:       usize,
}

/// What the storage needs to know about `Entry<C>` once `C` is erased.
#[derive(Clone, Copy)]
pub struct SharedComponentDescriptor
{
    pub type_id:      TypeId,
    pub entry_layout: Layout,
    key_offset:       usize,
    value_offset:     usize,
    fn_clone:         unsafe fn(*const u8, *mut u8),
    fn_drop:          fn(*mut u8),
}

impl SharedComponentDescriptor
{
    pub fn of<C: TSharedComponent>() -> Self
    {
        Self {
            type_id:      TypeId::of::<C>(),
            entry_layout: Layout::new::<Entry<C>>(),
            key_offset:   std::mem::offset_of!(Entry<C>, key),
            value_offset: std::mem::offset_of!(Entry<C>, value),
            fn_clone:     clone_entry::<C>,
            fn_drop:      drop_entry::<C>,
        }
    }

    /// The component registry entry for this type. A shared value never lives in a chunk, so
    /// the rest of the world only looks at its type id and its storage location.
    pub(crate) fn component_descriptor(&self) -> ComponentDescriptor
    {
        ComponentDescriptor {
            storage_type_id:  self.type_id,
            query_type_id:    self.type_id,
            byte_size:        self.entry_layout.size(),
            align:            self.entry_layout.align(),
            storage_location: StorageLocation::Archetype,
            fn_drop:          self.fn_drop,
        }
    }
}

unsafe fn clone_entry<C: TSharedComponent>(src: *const u8, dst: *mut u8)
{
    let src = unsafe { &*src.cast::<Entry<C>>() };
    // clone both halves before writing, so a panicking clone leaves `dst` untouched
    let entry = Entry::<C> {
        key:   src.key.clone(),
        value: src.value.clone(),
    };
    unsafe {
        dst.cast::<Entry<C>>().write(entry);
    }
}

fn drop_entry<C: TSharedComponent>(ptr: *mut u8)
{
    unsafe {
        std::ptr::drop_in_place(ptr.cast::<Entry<C>>());
    }
}

#[derive(Clone, Copy)]
struct SharedColumn
{
    value:      SharedValue,
    offset:     usize,
    descriptor: SharedComponentDescriptor,
}

pub struct ArchetypeChunk
{
    ptr:               NonNull<u8>,
    alloc_layout:      Layout,
    /// Sorted by component id, which is also the order the entries are written in.
    columns:           Vec<SharedColumn>,
    /// How many columns, counted from the front, hold a live entry. It is only smaller than
    /// `columns.len()` while `build` is still writing, so a panicking clone drops exactly the
    /// entries that were already written.
    initialized:       usize,
    component_bit_set: ComponentBitSet,
}

// SAFETY: `TSharedComponent` requires the value and its key to be `Send + Sync`. The entries are only
// reached through this struct, or through query views whose aliasing is ruled out by the query
// borrow and by the scheduler's access scopes, the same way chunk columns are.
unsafe impl Send for ArchetypeChunk {}
unsafe impl Sync for ArchetypeChunk {}

impl Default for ArchetypeChunk
{
    fn default() -> Self
    {
        Self {
            ptr:               NonNull::dangling(),
            alloc_layout:      Layout::new::<()>(),
            columns:           Vec::new(),
            initialized:       0,
            component_bit_set: ComponentBitSet::default(),
        }
    }
}

impl ArchetypeChunk
{
    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.columns.is_empty()
    }

    /// Component ids of the shared values held here
    #[inline]
    pub fn component_bit_set(&self) -> &ComponentBitSet
    {
        &self.component_bit_set
    }

    /// The shared values held here, ascending by component id
    pub fn values(&self) -> impl Iterator<Item = SharedValue> + '_
    {
        self.columns.iter().map(|column| column.value)
    }

    pub fn contains_value(&self, value: SharedValue) -> bool
    {
        self.column_of(value.component_id).is_some_and(|column| column.value == value)
    }

    pub fn value_of<C: TSharedComponent>(&self) -> Option<SharedValue>
    {
        self.column_of_type::<C>().map(|column| column.value)
    }

    pub fn key<C: TSharedComponent>(&self) -> Option<&C::Key>
    {
        let (key, _) = self.entry_of::<C>()?;
        Some(unsafe { &*key })
    }

    /// Pointers to the stored key and the value of `C`, or `None` when this archetype does not
    /// carry it. The storage never turns them into `&mut` itself: that is up to the caller, which
    /// knows nothing else writes the same value.
    pub(crate) fn entry_of<C: TSharedComponent>(&self) -> Option<(*const C::Key, *mut C)>
    {
        let column = self.column_of_type::<C>()?;
        unsafe {
            let entry = self.ptr.as_ptr().add(column.offset);
            Some((
                entry.add(column.descriptor.key_offset).cast::<C::Key>(),
                entry.add(column.descriptor.value_offset).cast::<C>(),
            ))
        }
    }

    /// Builds the storage of the archetype an entity moves to after a shared change.
    ///
    /// Every value of `src` except the one of type `C` is kept and cloned. `inserted`, when
    /// given, adds the new value of `C`; its key and value are moved in, not cloned.
    pub(crate) fn rebuild<C: TSharedComponent>(src: &Self, inserted: Option<(SharedValue, C::Key, C)>) -> Self
    {
        let inserted = inserted.map(|(shared_value, key, value)| {
            let write = move |slot: *mut u8| unsafe { slot.cast::<Entry<C>>().write(Entry::<C> { key, value }) };
            (shared_value, SharedComponentDescriptor::of::<C>(), write)
        });
        Self::build(src, Some(TypeId::of::<C>()), inserted)
    }

    fn build<W: FnOnce(*mut u8)>(src: &Self, excluded: Option<TypeId>, inserted: Option<(SharedValue, SharedComponentDescriptor, W)>) -> Self
    {
        let mut columns: Vec<SharedColumn> = src
            .columns
            .iter()
            .filter(|column| Some(column.descriptor.type_id) != excluded)
            .copied()
            .collect();
        let (inserted_id, mut write_inserted) = match inserted
        {
            Some((value, descriptor, write)) =>
            {
                columns.push(SharedColumn {
                    value:      value,
                    offset:     0,
                    descriptor: descriptor,
                });
                (Some(value.component_id), Some(write))
            }
            None => (None, None),
        };
        columns.sort_unstable_by_key(|column| column.value.component_id);

        let mut alloc_layout = Layout::new::<()>();
        let mut component_bit_set = ComponentBitSet::default();
        for column in columns.iter_mut()
        {
            let (extended, offset) = match alloc_layout.extend(column.descriptor.entry_layout)
            {
                Ok(r) => r,
                Err(e) => panic!("shared component layout overflow: {e}"),
            };
            alloc_layout = extended;
            column.offset = offset;
            component_bit_set.insert(column.value.component_id);
        }
        let alloc_layout = alloc_layout.pad_to_align();

        let ptr = if alloc_layout.size() == 0
        {
            // nothing to allocate, but zero-sized entries still need an aligned, non-null address
            NonNull::new(std::ptr::without_provenance_mut(alloc_layout.align())).unwrap()
        }
        else
        {
            match NonNull::new(unsafe { alloc(alloc_layout) })
            {
                Some(ptr) => ptr,
                None => handle_alloc_error(alloc_layout),
            }
        };

        let mut dst = Self {
            ptr:               ptr,
            alloc_layout:      alloc_layout,
            columns:           columns,
            initialized:       0,
            component_bit_set: component_bit_set,
        };
        for idx in 0..dst.columns.len()
        {
            let column = dst.columns[idx];
            let slot = unsafe { dst.ptr.as_ptr().add(column.offset) };
            match write_inserted.take_if(|_| inserted_id == Some(column.value.component_id))
            {
                Some(write) => write(slot),
                None =>
                {
                    let src_column = src.column_of(column.value.component_id).unwrap();
                    unsafe { (column.descriptor.fn_clone)(src.ptr.as_ptr().add(src_column.offset), slot) };
                }
            }
            dst.initialized += 1;
        }
        dst
    }

    fn column_of(&self, component_id: usize) -> Option<&SharedColumn>
    {
        match self.columns.binary_search_by_key(&component_id, |column| column.value.component_id)
        {
            Ok(idx) => Some(&self.columns[idx]),
            Err(_) => None,
        }
    }

    /// Linear, but an archetype rarely carries more than a handful of shared components
    fn column_of_type<C: TSharedComponent>(&self) -> Option<&SharedColumn>
    {
        let type_id = TypeId::of::<C>();
        self.columns.iter().find(|column| column.descriptor.type_id == type_id)
    }
}

impl Clone for ArchetypeChunk
{
    fn clone(&self) -> Self
    {
        Self::build(self, None, None::<(SharedValue, SharedComponentDescriptor, fn(*mut u8))>)
    }
}

impl Drop for ArchetypeChunk
{
    fn drop(&mut self)
    {
        for column in &self.columns[..self.initialized]
        {
            (column.descriptor.fn_drop)(unsafe { self.ptr.as_ptr().add(column.offset) });
        }
        if self.alloc_layout.size() != 0
        {
            unsafe {
                dealloc(self.ptr.as_ptr(), self.alloc_layout);
            }
        }
    }
}
