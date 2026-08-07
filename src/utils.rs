use std::any::TypeId;

use crate::apis::params::{ComponentSpec, ComponentSpecs};
use crate::apis::traits::{TComponent, TComponentDescriptor};
use crate::query::access_scope::AccessScope;
use crate::world::arch_spec::ArchetypeSpecs;
/// Resolves `T` to its registry index, registering it when the world has not seen it yet.
///
/// Registering here rather than erroring is what lets a query name a component no entity
/// carries: it simply matches no archetype. Skipping the component instead would leave an
/// empty bitset, which [`ComponentBitSet::contains_all`] reports as contained by *every*
/// archetype - the query would then match everything and read columns that do not exist.
pub(crate) fn component_id_for<T: TComponent + 'static>(component_specs: &mut ComponentSpecs) -> usize
{
    component_specs.get_or_insert_with(TypeId::of::<T::StorageType>(), || ComponentSpec {
        descriptor: T::COMPONENT_DESCRIPTOR,
    })
}

pub(crate) fn normalize_set(set: &mut Vec<usize>)
{
    set.sort();
    set.dedup();
}
/// Collects the *indices* of every archetype the scope matches. An index stays correct as the
/// registry grows, so a `QuerySpec` built here does not need re-pointing, only refreshing when
/// new archetypes appear.
pub(crate) fn build_archetype_which_contains(archetypes: &ArchetypeSpecs, dst: &mut Vec<usize>, access_scope: &AccessScope)
{
    for (idx, arch) in archetypes.values().enumerate()
    {
        if access_scope.belong_to(arch)
        {
            dst.push(idx);
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
