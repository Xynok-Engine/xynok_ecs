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

#[cfg(test)]
mod test
{
    use std::collections::HashSet;

    use crate::utils::normalize_set;

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
