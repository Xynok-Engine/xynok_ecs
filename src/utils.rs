use crate::{query::access_scope::AccessScope, world::arch_spec::ArchetypeSpec};
use std::collections::HashMap;

pub(crate) fn normalize_set(set: &mut Vec<usize>)
{
    set.sort();
    set.dedup();
}
pub(crate) fn build_archetype_which_contains(archetypes: &mut HashMap<usize, ArchetypeSpec>, dst: &mut Vec<*mut ArchetypeSpec>, access_scope: &AccessScope)
{
    for arch in archetypes.values_mut()
    {
        if access_scope.belong_to(arch)
        {
            let arch_ptr = arch as *mut ArchetypeSpec;
            dst.push(arch_ptr);
        }
    }
}
