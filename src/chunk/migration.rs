
use crate::chunk::column::StateOffset;
use crate::chunk::layout::ChunkLayout;

/// What happens to one column of the source archetype when an entity moves to the destination.
///
/// Two cases, matching the branches `Chunk::take_from` used to work out for itself with
/// a HashMap lookup per component per entity. Each one carries only the fields it actually
/// needs: there is no `dst_offset` on a column that is not going anywhere, and no `fn_drop` on
/// one nobody drops.
pub enum ComponentMigration
{
    Move(ColumnMove),
    Abandon(ColumnAbandon),
}

/// Where a column sits in the source chunk, and how big one element of it is.
///
/// Every case needs this much, if only to backfill the last row into the hole the moved entity
/// leaves behind, so both variants carry a copy.
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
    /// A column the destination also has is moved, state included, even when `merge_component`
    /// is about to overwrite it: the overwrite drops the old value and keeps enable and `added`
    /// (issue #43).
    pub fn new(src_layout: &ChunkLayout, dst_layout: &ChunkLayout) -> Self
    {
        let mut components = Vec::with_capacity(src_layout.columns.len());

        for src_col in src_layout.columns.iter()
        {
            let src = ColumnInSrc {
                offset:       src_col.offset,
                byte_size:    src_col.byte_size,
                state_offset: src_col.state_offset.clone(),
            };

            let migration = match dst_layout.component_col_descriptors.get(&src_col.storage_type_id)
            {
                Some(dst) => ComponentMigration::Move(ColumnMove {
                    src:              src,
                    dst_offset:       dst.offset,
                    dst_state_offset: dst.state_offset.clone(),
                }),
                None => ComponentMigration::Abandon(ColumnAbandon { src: src }),
            };

            components.push(migration);
        }

        Self { components: components }
    }
}
