use std::any::TypeId;

use crate::apis::custom_type::FnComponentDropItSelf;
use crate::chunk::column::StateOffset;
use crate::chunk::layout::ChunkLayout;

/// What happens to one column of the source archetype when an entity moves to the destination.
///
/// Three cases, matching the three branches `Chunk::take_from` used to work out for itself with
/// a HashMap lookup per component per entity. Each one carries only the fields it actually
/// needs: there is no `dst_offset` on a column that is not going anywhere, and no `fn_drop` on
/// one nobody drops.
pub enum ComponentMigration
{
    Move(ColumnMove),
    DropInPlace(ColumnDropInPlace),
    Abandon(ColumnAbandon),
}

/// Where a column sits in the source chunk, and how big one element of it is.
///
/// Every case needs this much, if only to backfill the last row into the hole the moved entity
/// leaves behind, so all three variants carry a copy.
#[derive(Clone)]
pub struct ColumnInSrc
{
    pub offset:       usize,
    /// Bytes per element, from `ComponentDescriptor::byte_size`.
    pub byte_size:    usize,
    /// Offsets of enable, added and changed on the source side.
    pub state_offset: StateOffset,
}

/// The destination carries this column too, so value and state travel with the entity.
pub struct ColumnMove
{
    pub src:              ColumnInSrc,
    /// Offset of the column in the destination chunk.
    pub dst_offset:       usize,
    /// The matching state offsets on the destination side.
    pub dst_state_offset: StateOffset,
}

/// The caller is about to overwrite this column with a new value (`merge_component`), so the old
/// one has to be dropped in place rather than carried over, otherwise it leaks silently.
pub struct ColumnDropInPlace
{
    pub src:     ColumnInSrc,
    /// `ComponentDescriptor::fn_drop`.
    pub fn_drop: FnComponentDropItSelf,
}

/// The destination has no such column and the caller already read the value out
/// (`remove_component`), so all that is left is backfilling the last row. No drop, no copy.
pub struct ColumnAbandon
{
    pub src: ColumnInSrc,
}

impl ComponentMigration
{
    /// The source-side position, which every case needs for the backfill.
    #[inline]
    pub fn src(&self) -> &ColumnInSrc
    {
        match self
        {
            Self::Move(e) => &e.src,
            Self::DropInPlace(e) => &e.src,
            Self::Abandon(e) => &e.src,
        }
    }
}

/// Everything that has to happen when an entity moves from one archetype to another.
///
/// Built once and kept in `World`'s edge cache, which leaves `take_from` as a plain loop over a
/// slice: no hashing, no `ComponentSpecs` lookup, no reaching into the destination layout.
pub struct MigrationPlan
{
    pub components: Vec<ComponentMigration>,
}

impl MigrationPlan
{
    /// `overwritten_type_ids` are the components the caller overwrites as soon as the move is
    /// done, i.e. `merge_component`'s `T::STORAGE_TYPE_IDS`. `remove_component` passes an empty
    /// slice, since it has already read those values out.
    pub fn new(src_layout: &ChunkLayout, dst_layout: &ChunkLayout, overwritten_type_ids: &[TypeId]) -> Self
    {
        let mut components = Vec::with_capacity(src_layout.columns.len());

        for src_col in src_layout.columns.iter()
        {
            let src = ColumnInSrc {
                offset:       src_col.offset,
                byte_size:    src_col.byte_size,
                state_offset: src_col.state_offset.clone(),
            };

            let migration = match overwritten_type_ids.contains(&src_col.storage_type_id)
            {
                true => ComponentMigration::DropInPlace(ColumnDropInPlace {
                    src:     src,
                    fn_drop: src_col.fn_drop,
                }),
                false => match dst_layout.component_col_descriptors.get(&src_col.storage_type_id)
                {
                    Some(dst) => ComponentMigration::Move(ColumnMove {
                        src:              src,
                        dst_offset:       dst.offset,
                        dst_state_offset: dst.state_offset.clone(),
                    }),
                    None => ComponentMigration::Abandon(ColumnAbandon { src: src }),
                },
            };

            components.push(migration);
        }

        Self { components: components }
    }
}
