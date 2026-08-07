use crate::apis::identifies::XynokEcsError;
use crate::collection::component_bit_set::ComponentBitSet;
use crate::world::arch_spec::ArchetypeSpec;

#[derive(Default, Clone)]
pub struct AccessScope
{
    pub read:    ComponentBitSet,
    pub write:   ComponentBitSet,
    pub exclude: ComponentBitSet,
}
impl AccessScope
{
    pub fn extend(&mut self, other: AccessScope) -> Result<(), XynokEcsError>
    {
        if self.collide_with(&other)
        {
            return Err(XynokEcsError::QueryAccessScopeConflict);
        }
        self.read.union_with(&other.read);
        self.write.union_with(&other.write);
        self.exclude.union_with(&other.exclude);
        Ok(())
    }

    pub fn is_read_only(&self) -> bool
    {
        self.write.is_empty()
    }

    pub fn belong_to(&self, arch: &ArchetypeSpec) -> bool
    {
        arch.contains_all_type_id_components_of(&self.read) && arch.contains_all_type_id_components_of(&self.write)
    }

    pub fn collide_with(&self, other: &AccessScope) -> bool
    {
        self.read.iter().any(|e| other.write.contains(e)) // read while other write
        || self.write.iter().any(|e| other.read.contains(e)) // write while other read
        || self.write.iter().any(|e| other.write.contains(e)) // both write on the same
    }

    /// Whether two systems may run at the same time.
    ///
    /// The data conflicts are the same ones [`Self::collide_with`] rejects within a single
    /// system: one side writing what the other reads or writes. Shared reads are fine.
    ///
    /// Exclusion adds a case that `collide_with` has no reason to consider. If one scope
    /// excludes a component the other requires, the two can never match the same archetype,
    /// so they can never reach the same row - and that holds even when both write the same
    /// component.
    pub fn can_parallel_with(&self, other: &AccessScope) -> bool
    {
        if self.matches_archetypes_disjoint_from(other) || other.matches_archetypes_disjoint_from(self)
        {
            return true;
        }
        !self.collide_with(other)
    }

    /// `self` only matches archetypes lacking a component that `other` insists on, so the two
    /// select disjoint sets of archetypes
    fn matches_archetypes_disjoint_from(&self, other: &AccessScope) -> bool
    {
        self.exclude.intersects(&other.read) || self.exclude.intersects(&other.write)
    }
}

#[cfg(test)]
mod test
{
    use super::AccessScope;
    use crate::collection::component_bit_set::ComponentBitSet;

    fn bits(ids: &[usize]) -> ComponentBitSet
    {
        let mut set = ComponentBitSet::default();
        for id in ids
        {
            set.insert(*id);
        }
        set
    }

    fn scope(read: &[usize], write: &[usize], exclude: &[usize]) -> AccessScope
    {
        AccessScope {
            read:    bits(read),
            write:   bits(write),
            exclude: bits(exclude),
        }
    }

    fn ids(set: &ComponentBitSet) -> Vec<usize>
    {
        set.iter().collect()
    }

    #[test]
    fn extend_merges_every_set_into_its_own_kind()
    {
        let mut a = scope(&[0], &[2], &[5]);
        a.extend(scope(&[1], &[3], &[7])).expect("disjoint scopes must merge");

        assert_eq!(ids(&a.read), [0, 1]);
        assert_eq!(ids(&a.write), [2, 3]);
        assert_eq!(ids(&a.exclude), [5, 7], "exclude must come from the other scope's exclude, not its write");
    }

    #[test]
    fn extend_rejects_a_read_write_conflict()
    {
        let mut a = scope(&[0], &[], &[]);
        assert!(a.extend(scope(&[], &[0], &[])).is_err(), "reading what the other writes must conflict");

        let mut b = scope(&[], &[1], &[]);
        assert!(b.extend(scope(&[], &[1], &[])).is_err(), "two writers of the same component must conflict");
    }

    #[test]
    fn extend_allows_two_readers_of_the_same_component()
    {
        let mut a = scope(&[4], &[], &[]);
        a.extend(scope(&[4], &[], &[])).expect("shared reads must be allowed");
        assert_eq!(ids(&a.read), [4]);
    }

    #[test]
    fn parallel_when_nothing_is_shared()
    {
        assert!(scope(&[0], &[1], &[]).can_parallel_with(&scope(&[2], &[3], &[])));
        assert!(scope(&[], &[], &[]).can_parallel_with(&scope(&[], &[], &[])), "two systems touching nothing");
    }

    #[test]
    fn parallel_when_both_only_read()
    {
        assert!(scope(&[0, 1], &[], &[]).can_parallel_with(&scope(&[1, 2], &[], &[])));
    }

    #[test]
    fn not_parallel_when_one_writes_what_the_other_touches()
    {
        assert!(!scope(&[0], &[], &[]).can_parallel_with(&scope(&[], &[0], &[])), "read vs write");
        assert!(!scope(&[], &[0], &[]).can_parallel_with(&scope(&[0], &[], &[])), "write vs read");
        assert!(!scope(&[], &[0], &[]).can_parallel_with(&scope(&[], &[0], &[])), "write vs write");
    }

    /// The case `collide_with` alone would get wrong: both write component 0, but one only
    /// matches archetypes carrying component 1 while the other only matches those without it
    #[test]
    fn parallel_when_exclusion_makes_the_archetype_sets_disjoint()
    {
        let without_1 = scope(&[], &[0], &[1]);
        let with_1 = scope(&[1], &[0], &[]);

        assert!(without_1.collide_with(&with_1), "they do write the same component");
        assert!(without_1.can_parallel_with(&with_1), "yet no archetype can match both");
        assert!(with_1.can_parallel_with(&without_1), "and the check is symmetric");
    }

    #[test]
    fn exclusion_of_an_untouched_component_does_not_grant_parallelism()
    {
        // 9 is excluded by one side but neither read nor written by the other, so it says
        // nothing about whether their archetype sets overlap
        let a = scope(&[], &[0], &[9]);
        let b = scope(&[], &[0], &[]);
        assert!(!a.can_parallel_with(&b));
    }
}
