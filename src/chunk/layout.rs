use std::alloc::Layout;
use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::constants::{BITS_PER_BYTE, CHUNK_SIZE_IN_BYTE, CPU_WORD};
use crate::apis::identifies::XynokEcsError;
use crate::apis::traits::TComponentDescriptor;
use crate::apis::ComponentDescriptor;
use crate::chunk::column::ColumnDescriptor;
use crate::chunk::header::Header;
use crate::entity::Entity;
use crate::utils::align_up;

pub struct ChunkLayout
{
    pub max_len:                   usize,
    pub header:                    Header,
    pub alloc_layout:              Layout,
    pub component_col_descriptors: HashMap<TypeId, ColumnDescriptor>,
}

pub struct ChunkLayoutParams<'a>
{
    pub components:                 &'a [ComponentDescriptor],
    pub component_descriptors_temp: &'a mut HashMap<TypeId, ColumnDescriptor>,
}

impl ChunkLayout
{
    pub fn new(mut params: ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        #[cfg(debug_assertions)]
        {
            use crate::apis::identifies::StorageLocation;
            if params.components.iter().any(|e| e.storage_location != StorageLocation::Chunk)
            {
                return Err(XynokEcsError::ThisArchetypeContainsShareAbleComponent);
            }
        }

        let result = compute_layout(&mut params)?;
        Ok(result)
    }
}
fn compute_layout(params: &mut ChunkLayoutParams) -> Result<ChunkLayout, XynokEcsError>
{
    // Each entity costs its handle in the header plus one slot in every component column
    let bytes_per_entity = params
        .components
        .iter()
        .fold(Entity::COMPONENT_DESCRIPTOR.byte_size, |acc, des| acc.saturating_add(des.byte_size));

    // arch.len() represents the number of components. We use this count to store the
    // enabled/disabled state of each component as a single bit
    let bits_per_entity = bytes_per_entity.saturating_mul(BITS_PER_BYTE).saturating_add(params.components.len());

    let mut max_entities = (CHUNK_SIZE_IN_BYTE * BITS_PER_BYTE) / bits_per_entity;

    loop
    {
        if max_entities == 0
        {
            return Err(XynokEcsError::ArchetypeIsTooLarge);
        }

        if let Ok(valid_layout) = try_layout(max_entities, params)
        {
            return Ok(valid_layout);
        }
        max_entities -= 1;
    }
}
/// Attempts to build a layout for `max_entities` rows, returns `None` if the total size exceeds [`CHUNK_SIZE_IN_BYTE`]
fn try_layout(max_entities: usize, params: &mut ChunkLayoutParams) -> Result<ChunkLayout, XynokEcsError>
{
    let header = Header::new(max_entities, params.components.len());
    let mut cursor = header.size;
    // Header's own bitset requires CPU_WORD alignment, so this is the floor even when
    // the archetype has no components (and thus no des.align to fold over)
    let mut max_align = CPU_WORD;
    params.component_descriptors_temp.clear();

    for des in params.components
    {
        cursor = align_up(cursor, des.align);

        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return Err(XynokEcsError::ArchetypeIsTooLarge);
        }
        params.component_descriptors_temp.insert(des.storage_type_id, des.as_column_descriptor(cursor));

        let column_bytes = match des.byte_size.checked_mul(max_entities)
        {
            Some(r) => r,
            None => return Err(XynokEcsError::ArchetypeIsTooLarge),
        };
        cursor = match cursor.checked_add(column_bytes)
        {
            Some(r) => r,
            None => return Err(XynokEcsError::ArchetypeIsTooLarge),
        };
        if cursor > CHUNK_SIZE_IN_BYTE
        {
            return Err(XynokEcsError::ArchetypeIsTooLarge);
        }
        max_align = max_align.max(des.align);
    }
    let alloc_layout = match Layout::from_size_align(CHUNK_SIZE_IN_BYTE, max_align)
    {
        Ok(l) => l,
        Err(e) => return Err(XynokEcsError::ChunkLayoutAllocation(e)),
    };
    Ok(ChunkLayout {
        max_len:                   max_entities,
        component_col_descriptors: params.component_descriptors_temp.clone(),
        header:                    header,
        alloc_layout:              alloc_layout,
    })
}

#[cfg(test)]
mod test
{
    use std::collections::HashMap;

    use super::*;
    use crate::apis::identifies::StorageLocation;

    macro_rules! declare_component {
        ($ty:ty) => {
            impl crate::apis::traits::TComponent for $ty
            {
                type QueryType = Self;
                type StorageType = Self;

                const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;
            }
        };
    }

    struct Hp(#[allow(unused)] u32);
    declare_component!(Hp);

    struct Mana(#[allow(unused)] u32);
    declare_component!(Mana);

    struct Pos
    {
        #[allow(unused)]
        x: f32,
        #[allow(unused)]
        y: f32,
    }
    declare_component!(Pos);

    /// Zero-sized component: every row shares the same offset and `byte_size == 0`.
    struct Marker;
    declare_component!(Marker);

    /// Over-aligned component, to check that column offsets honour `align_of`.
    #[repr(align(32))]
    struct Aligned32(#[allow(unused)] u64);
    declare_component!(Aligned32);

    fn layout_of(descriptors: &[ComponentDescriptor]) -> Result<ChunkLayout, XynokEcsError>
    {
        let mut temp = HashMap::new();
        ChunkLayout::new(ChunkLayoutParams {
            components:                 descriptors,
            component_descriptors_temp: &mut temp,
        })
    }

    #[test]
    fn column_offsets_respect_alignment()
    {
        let descriptors = [Hp::COMPONENT_DESCRIPTOR, Aligned32::COMPONENT_DESCRIPTOR, Pos::COMPONENT_DESCRIPTOR];
        let layout = layout_of(&descriptors).expect("layout must be constructible");

        for descriptor in &descriptors
        {
            let column = layout
                .component_col_descriptors
                .get(&descriptor.query_type_id)
                .expect("every component of the archetype must own a column");
            assert_eq!(
                column.offset % descriptor.align,
                0,
                "column at offset {} violates align {}",
                column.offset,
                descriptor.align
            );
        }
    }

    #[test]
    fn columns_do_not_overlap_and_stay_inside_the_chunk()
    {
        let descriptors = [Hp::COMPONENT_DESCRIPTOR, Mana::COMPONENT_DESCRIPTOR, Pos::COMPONENT_DESCRIPTOR];
        let layout = layout_of(&descriptors).expect("layout must be constructible");

        let mut spans: Vec<(usize, usize)> = descriptors
            .iter()
            .map(|descriptor| {
                let column = layout.component_col_descriptors.get(&descriptor.query_type_id).unwrap();
                (column.offset, column.offset + descriptor.byte_size * layout.max_len)
            })
            .collect();
        spans.sort();

        let header_end = layout.header.entities_offset + layout.max_len * size_of::<Entity>();
        assert!(
            spans[0].0 >= header_end,
            "first column at {} overlaps the header ending at {}",
            spans[0].0,
            header_end
        );

        for pair in spans.windows(2)
        {
            assert!(pair[0].1 <= pair[1].0, "columns {:?} and {:?} overlap", pair[0], pair[1]);
        }
        assert!(
            spans.last().unwrap().1 <= CHUNK_SIZE_IN_BYTE,
            "last column ends at {} which exceeds the {CHUNK_SIZE_IN_BYTE} byte chunk",
            spans.last().unwrap().1
        );
    }

    #[test]
    fn uses_the_chunk_efficiently()
    {
        let descriptors = [Hp::COMPONENT_DESCRIPTOR, Mana::COMPONENT_DESCRIPTOR];
        let layout = layout_of(&descriptors).expect("layout must be constructible");

        // `header.size` already covers the enable/disable bitset and the entity column,
        // so only the component columns are counted on top of it.
        let bytes_per_row = size_of::<Hp>() + size_of::<Mana>();
        let used = layout.header.size + bytes_per_row * layout.max_len;

        assert!(used <= CHUNK_SIZE_IN_BYTE, "layout claims {used} bytes for a {CHUNK_SIZE_IN_BYTE} byte chunk");
        assert!(
            used * 100 / CHUNK_SIZE_IN_BYTE >= 90,
            "layout wastes too much of the chunk: {used}/{CHUNK_SIZE_IN_BYTE} bytes used for max_len = {}",
            layout.max_len
        );
    }

    #[test]
    fn supports_zero_sized_components()
    {
        let layout = layout_of(&[Marker::COMPONENT_DESCRIPTOR]).expect("a ZST-only archetype must be constructible");
        assert!(layout.max_len > 0, "a ZST archetype must still hold rows");
        assert!(layout.component_col_descriptors.contains_key(&Marker::COMPONENT_DESCRIPTOR.query_type_id));
    }

    #[test]
    fn header_reserves_room_for_the_entity_column()
    {
        let layout = layout_of(&[Hp::COMPONENT_DESCRIPTOR]).expect("layout must be constructible");
        let entities_end = layout.header.entities_offset + layout.max_len * size_of::<Entity>();

        assert!(
            entities_end <= layout.header.size,
            "entity column ends at {entities_end} but the header is only {} bytes",
            layout.header.size
        );
        assert_eq!(
            layout.header.entities_offset % align_up(align_of::<Entity>(), align_of::<Entity>()),
            0,
            "the entity column must be aligned"
        );
    }
}
