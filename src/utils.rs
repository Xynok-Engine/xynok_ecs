use crate::apis::identifies::StorageLocation;
use crate::apis::traits::TArchetype;
use crate::apis::ComponentDescriptor;
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpec;
use std::collections::HashMap;

pub(crate) fn build_component_descriptors_with<T: TArchetype + 'static>(storage_location: StorageLocation, dst: &mut Vec<ComponentDescriptor>)
{
    dst.clear();
    for e in T::COMPONENT_DESCRIPTORS
    {
        if e.storage_location == storage_location
        {
            dst.push(e.clone());
        }
    }
}
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
/// Rounds up `offset` to the nearest multiple of `align` (align must be a power of 2)
#[inline(always)]
pub const fn align_up(offset: usize, align: usize) -> usize
{
    (offset + align - 1) & !(align - 1)
}
#[cfg(test)]
mod test
{
    use std::collections::HashSet;
    #[test]
    fn t_align_up()
    {
        assert!(align_up(45, 64) == 64);
        assert!(align_up(2, 8) == 8);
        assert!(align_up(8, 8) == 8);
        assert!(align_up(9, 8) == 16);
        assert!(align_up(1024, 64) == 1024);
        assert!(align_up(1021, 64) == 1024);
    }

    use crate::utils::{align_up, normalize_set};

    #[test]
    fn unique_component_set()
    {
        let mut a = Vec::from([1, 2, 3]);
        let mut b = Vec::from([1, 3, 2]);
        let mut c = Vec::from([1, 3, 2, 3]);
        normalize_set(&mut a);
        normalize_set(&mut b);
        normalize_set(&mut c);
        let dict = HashSet::from([a, b, c]);
        assert!(dict.len() == 1);

        let mut d = Vec::from([1, 1, 2, 3, 2, 3]);
        assert!(!dict.contains(&d));
        normalize_set(&mut d);
        assert!(dict.contains(&d));
        println!("dict: {:?}", dict);
    }
}
