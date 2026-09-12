use std::alloc::Layout;
use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::constants::{BITS_PER_BYTE, CHANGED_TICK_BYTE_SIZE, CHUNK_SIZE_IN_BYTE, CPU_WORD};
use crate::apis::identifies::{StateDetection, XynokEcsError};
use crate::apis::params::ComponentSpecs;
use crate::apis::traits::TComponentDescriptor;
use crate::apis::ComponentDescriptor;
use crate::chunk::column::{ColumnDescriptor, StateOffset};
use crate::chunk::header::Header;
use crate::collection::component_bit_set::ComponentBitSet;
use crate::entity::Entity;
use crate::utils::align_up;

pub struct ChunkLayout
{
    pub max_len:                   usize,
    pub header:                    Header,
    pub alloc_layout:              Layout,
    pub component_bit_set:         ComponentBitSet,
    pub component_col_descriptors: HashMap<TypeId, ColumnDescriptor>,
}

pub struct ChunkLayoutParams<'a>
{
    pub components:                 &'a [ComponentDescriptor],
    pub component_specs:            &'a ComponentSpecs,
    pub state_offsets_temp:         &'a mut HashMap<TypeId, StateOffset>,
    pub component_descriptors_temp: &'a mut HashMap<TypeId, ColumnDescriptor>,
    pub component_bit_set_temp:     &'a mut ComponentBitSet,
}

impl ChunkLayout
{
    pub fn new(mut params: ChunkLayoutParams) -> Result<Self, XynokEcsError>
    {
        //#[cfg(debug_assertions)]
        //{
        //    use crate::apis::identifies::StorageLocation;
        //    if params.components.iter().any(|e| e.storage_location != StorageLocation::Chunk)
        //    {
        //        return Err(XynokEcsError::ThisArchetypeContainsShareAbleComponent);
        //    }
        //}

        let result = compute_layout(&mut params)?;
        Ok(result)
    }
}
fn compute_layout(params: &mut ChunkLayoutParams) -> Result<ChunkLayout, XynokEcsError>
{
    build_component_bit_set(params.component_bit_set_temp, params.components, params.component_specs)?;

    let mut upper_bound = estimate_max_entities(params.components);

    // The estimate ignores padding, so it can only be too generous, never too small. That makes
    // it a valid upper bound for the search below.
    let mut best: Option<ChunkLayout> = None;
    let mut low = 1usize;

    while low <= upper_bound
    {
        let mid = low + (upper_bound - low) / 2;
        // `try_layout` is monotonic: every part of the layout grows with the row count, so once
        // a size fits, every smaller size fits too. Binary search is therefore valid, and it
        // replaces the old `max_entities -= 1` walk that could burn hundreds of rounds of
        // HashMap rebuilding per archetype.
        match try_layout(mid, params)
        {
            Ok(layout) =>
            {
                best = Some(layout);
                low = mid + 1;
            }
            // Only "does not fit" narrows the range. Anything else is a real problem with the
            // archetype itself and would say the same at every row count, so it goes straight out.
            Err(XynokEcsError::ArchetypeIsTooLarge) =>
            {
                upper_bound = mid - 1;
            }
            Err(e) => return Err(e),
        }
    }

    match best
    {
        Some(layout) => Ok(layout),
        None => Err(XynokEcsError::ArchetypeIsTooLarge),
    }
}
/// Upper bound on how many rows a chunk can hold, counting everything that costs bytes per row.
///
/// The old version only counted the component payloads plus one enable bit each, which left out
/// the `added` and `changed` ticks entirely. For a change-tracked archetype that is 8 bytes per
/// row per component missing, so the starting guess came out several hundred rows too high and
/// the search had to walk all the way down.
fn estimate_max_entities(components: &[ComponentDescriptor]) -> usize
{
    // Each entity costs its handle in the header plus one slot in every component column
    let mut bits_per_entity = Entity::COMPONENT_DESCRIPTOR.byte_size.saturating_mul(BITS_PER_BYTE);

    for des in components
    {
        bits_per_entity = bits_per_entity.saturating_add(des.byte_size.saturating_mul(BITS_PER_BYTE));

        // Enable state is a single bit per entity, change tracking is two ticks (`added` and
        // `changed`). Both live in the header, both scale with the row count.
        if matches!(des.state_detection, StateDetection::EnableAble | StateDetection::EnableAbleAndChangeAble)
        {
            bits_per_entity = bits_per_entity.saturating_add(1);
        }
        if matches!(des.state_detection, StateDetection::ChangeAble | StateDetection::EnableAbleAndChangeAble)
        {
            bits_per_entity = bits_per_entity.saturating_add(CHANGED_TICK_BYTE_SIZE.saturating_mul(2).saturating_mul(BITS_PER_BYTE));
        }
    }

    if bits_per_entity == 0
    {
        return 0;
    }
    (CHUNK_SIZE_IN_BYTE * BITS_PER_BYTE) / bits_per_entity
}
/// Builds the archetype's component bit set, and rejects a component named twice on the way.
///
/// The bit set is the natural place for that check: a component already carrying its bit is
/// exactly a duplicate, so it costs one `contains` per component and no extra storage. The
/// world catches this earlier (see `check_no_duplicate_component`), this is the backstop for
/// the layouts built by merging two archetypes.
fn build_component_bit_set(dst: &mut ComponentBitSet, src: &[ComponentDescriptor], component_specs: &ComponentSpecs) -> Result<(), XynokEcsError>
{
    dst.clear();
    for des in src
    {
        match component_specs.index_of(&des.storage_type_id)
        {
            Some(component_id) =>
            {
                if dst.contains(component_id)
                {
                    return Err(XynokEcsError::DuplicateComponentInArchetype(des.name()));
                }
                dst.insert(component_id)
            }
            None => return Err(XynokEcsError::ComponentSpecIsNotRegistered),
        }
    }
    Ok(())
}
/// Attempts to build a layout for `max_entities` rows, returns `None` if the total size exceeds [`CHUNK_SIZE_IN_BYTE`]
fn try_layout(max_entities: usize, params: &mut ChunkLayoutParams) -> Result<ChunkLayout, XynokEcsError>
{
    let header = Header::new(max_entities, params.components, params.state_offsets_temp);
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
        // `Header::new` seeds one entry per component, and `compute_layout` has already
        // rejected duplicates, so the only way this comes up empty is a component appearing
        // twice. Reported rather than unwrapped: the panic it used to raise said nothing.
        let state_offset = match params.state_offsets_temp.remove(&des.storage_type_id)
        {
            Some(r) => r,
            None => return Err(XynokEcsError::DuplicateComponentInArchetype(des.name())),
        };

        params
            .component_descriptors_temp
            .insert(des.storage_type_id, des.as_column_descriptor(cursor, state_offset));

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
        component_bit_set:         params.component_bit_set_temp.clone(),
        header:                    header,
        alloc_layout:              alloc_layout,
    })
}

#[cfg(test)]
mod test
{
    use std::collections::HashMap;

    use super::*;
    use crate::apis::identifies::{StateDetection, StorageLocation};
    use crate::apis::params::ComponentSpec;

    macro_rules! declare_component {
        ($ty:ty) => {
            impl crate::apis::traits::TComponent for $ty
            {
                type QueryType = Self;
                type StorageType = Self;
                type MutPolicy = crate::query::mut_ref::NoTracking;

                const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;

                const STATE_DETECTION: StateDetection = StateDetection::None;
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

    /// Change-tracked component: costs two ticks per row in the header on top of its payload.
    struct Tracked(#[allow(unused)] u32);
    impl crate::apis::traits::TComponent for Tracked
    {
        type QueryType = Self;
        type StorageType = Self;
        type MutPolicy = crate::query::mut_ref::NoTracking;

        const STORAGE_LOCATION: StorageLocation = StorageLocation::Chunk;

        const STATE_DETECTION: StateDetection = StateDetection::ChangeAble;
    }

    fn specs_of(descriptors: &[ComponentDescriptor]) -> ComponentSpecs
    {
        let mut specs = ComponentSpecs::new();
        for des in descriptors
        {
            specs.insert(des.storage_type_id, ComponentSpec { descriptor: des.clone() });
        }
        specs
    }

    fn layout_with(descriptors: &[ComponentDescriptor], specs: &ComponentSpecs) -> Result<ChunkLayout, XynokEcsError>
    {
        let mut temp = HashMap::new();
        let mut offset = HashMap::new();
        let mut bit_set = ComponentBitSet::default();
        ChunkLayout::new(ChunkLayoutParams {
            components:                 descriptors,
            component_specs:            specs,
            state_offsets_temp:         &mut offset,
            component_descriptors_temp: &mut temp,
            component_bit_set_temp:     &mut bit_set,
        })
    }

    fn layout_of(descriptors: &[ComponentDescriptor]) -> Result<ChunkLayout, XynokEcsError>
    {
        layout_with(descriptors, &specs_of(descriptors))
    }


    /// The row-count estimate feeds a binary search, so it has to be an upper bound (otherwise
    /// the search silently settles for a smaller chunk) and it has to be tight (otherwise the
    /// search wastes rounds getting back down). Change-tracked components are the case the old
    /// estimate got wrong: it counted the payload but not the `added`/`changed` ticks.
    #[test]
    fn estimate_is_a_tight_upper_bound()
    {
        let sets: [&[ComponentDescriptor]; 3] = [
            &[Tracked::COMPONENT_DESCRIPTOR],
            &[Hp::COMPONENT_DESCRIPTOR, Mana::COMPONENT_DESCRIPTOR],
            &[Tracked::COMPONENT_DESCRIPTOR, Hp::COMPONENT_DESCRIPTOR, Pos::COMPONENT_DESCRIPTOR],
        ];

        for descriptors in sets
        {
            let estimate = estimate_max_entities(descriptors);
            let actual = layout_of(descriptors).expect("layout must be constructible").max_len;

            assert!(estimate >= actual, "estimate {estimate} is below the real capacity {actual}, the search would undershoot");
            assert!(
                estimate - actual <= actual / 20,
                "estimate {estimate} is more than 5% above the real capacity {actual}, the search pays for the slack"
            );
        }
    }

    #[test]
    fn bit_set_holds_exactly_the_registry_ids_of_its_components()
    {
        // A registry carrying more than this archetype uses, so the ids are not 0..n
        let registry = specs_of(&[
            Mana::COMPONENT_DESCRIPTOR,
            Hp::COMPONENT_DESCRIPTOR,
            Pos::COMPONENT_DESCRIPTOR,
            Marker::COMPONENT_DESCRIPTOR,
        ]);
        let descriptors = [Hp::COMPONENT_DESCRIPTOR, Pos::COMPONENT_DESCRIPTOR];
        let layout = layout_with(&descriptors, &registry).expect("layout must be constructible");

        let mut expected: Vec<_> = descriptors
            .iter()
            .map(|des| registry.index_of(&des.storage_type_id).expect("registered above"))
            .collect();
        expected.sort();

        let got: Vec<_> = layout.component_bit_set.iter().collect();
        assert_eq!(got, expected, "bit set must carry exactly the archetype's component ids");
    }

    /// A layout built by merging two archetypes must not end up with two columns for one
    /// component. The world rejects this earlier, but the layout is the last line of defence:
    /// the second column would silently overwrite the first without dropping it.
    #[test]
    fn duplicate_component_is_rejected()
    {
        let descriptors = [Hp::COMPONENT_DESCRIPTOR, Mana::COMPONENT_DESCRIPTOR, Hp::COMPONENT_DESCRIPTOR];
        let result = layout_of(&descriptors);

        match result
        {
            Err(XynokEcsError::DuplicateComponentInArchetype(name)) => assert!(name.ends_with("Hp"), "the message must name the offender, got `{name}`"),
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("an archetype naming `Hp` twice must not produce a layout"),
        }
    }

    #[test]
    fn unregistered_component_is_rejected()
    {
        let empty = ComponentSpecs::new();
        let result = layout_with(&[Hp::COMPONENT_DESCRIPTOR], &empty);
        assert!(
            matches!(result, Err(XynokEcsError::ComponentSpecIsNotRegistered)),
            "a component with no registry id cannot be given a bit"
        );
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
