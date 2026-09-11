use std::any::TypeId;
use std::collections::HashMap;

use crate::apis::constants::{BITS_PER_BYTE, CHANGED_TICK_BYTE_SIZE, CPU_WORD};
use crate::apis::identifies::StateDetection;
use crate::apis::traits::TComponentDescriptor;
use crate::apis::ComponentDescriptor;
use crate::chunk::column::StateOffset;
use crate::entity::Entity;
use crate::utils::align_up;

pub struct Header
{
    pub entities_offset: usize,
    pub size:            usize,
}
impl Header
{
    pub fn new(max_entities: usize, components: &[ComponentDescriptor], state_offsets: &mut HashMap<TypeId, StateOffset>) -> Self
    {
        let mut offset = 0usize;
        state_offsets.clear();
        for e in components.iter()
        {
            let needs_enable = matches!(e.state_detection, StateDetection::EnableAble | StateDetection::EnableAbleAndChangeAble);
            let needs_change = matches!(e.state_detection, StateDetection::ChangeAble | StateDetection::EnableAbleAndChangeAble);

            let mut state_offset = StateOffset::default();

            if needs_enable
            {
                // enabled bit: 1 bit per entity
                state_offset.enable_offset = Some(offset);
                let enable_bytes = align_up(max_entities.div_ceil(BITS_PER_BYTE), CPU_WORD);
                offset = align_up(offset + enable_bytes, CPU_WORD);
            }

            if needs_change
            {
                // added tick: 1 tick per entity. Same tick type as `changed`, both set together
                // on insert; only `changed` moves afterwards on mutation. A bit can't tell two
                // independent readers (different last-run ticks) apart, same reason `changed`
                // is a tick and not a bit.
                state_offset.added_offset = Some(offset);
                let added_bytes = align_up(max_entities * CHANGED_TICK_BYTE_SIZE, CPU_WORD);
                offset = align_up(offset + added_bytes, CPU_WORD);

                // changed tick: 1 tick per entity
                state_offset.changed_offset = Some(offset);
                let changed_bytes = align_up(max_entities * CHANGED_TICK_BYTE_SIZE, CPU_WORD);
                offset = align_up(offset + changed_bytes, CPU_WORD);
            }

            state_offsets.insert(e.storage_type_id, state_offset);
        }
        // entities
        let entities_offset = align_up(offset, Entity::COMPONENT_DESCRIPTOR.align);
        let entities_size = max_entities * Entity::COMPONENT_DESCRIPTOR.byte_size;

        let size = align_up(entities_offset + entities_size, CPU_WORD);
        Self {
            entities_offset: entities_offset,
            size:            size,
        }
    }
}
