use crate::apis::identifies::XynokEcsError;
use crate::collection::component_bit_set::ComponentBitSet;
use crate::world::arch_spec::ArchetypeSpec;

/// What one query touches, and how.
///
/// Atomic on purpose: all three sets describe a single row selector. A system holding several
/// queries gets several of these - see [`AccessScopes`] for why they must not be merged.
#[derive(Default, Clone)]
pub struct AccessScope
{
    pub read:    ComponentBitSet,
    pub write:   ComponentBitSet,
    pub exclude: ComponentBitSet,
}
impl AccessScope
{
    /// Folds another element of the *same* query in, e.g. the `&Mana` of `Query<(&Hp, &Mana)>`.
    ///
    /// Union is the correct merge here and only here: the elements of one query all resolve
    /// against one row, so there is no pairing between `exclude` and `read`/`write` left to
    /// lose. For the same reason no disjointness escape applies - `Query<(&Hp, &mut Hp)>` hands
    /// out `&Hp` and `&mut Hp` for the same row whatever the filters say, so this stays a bare
    /// [`Self::collide_with`] check rather than [`Self::conflicts_with`].
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

    /// Whether `arch` is one of the archetypes this scope iterates.
    ///
    /// The `exclude` half is not decoration: [`Self::matches_archetypes_disjoint_from`] proves
    /// two scopes can never meet by pointing at a component one of them excludes, and that
    /// proof is only worth anything if selection here actually honours it.
    pub fn belong_to(&self, arch: &ArchetypeSpec) -> bool
    {
        arch.contains_all_type_id_components_of(&self.read)
            && arch.contains_all_type_id_components_of(&self.write)
            && !arch.intersects_type_id_components_of(&self.exclude)
    }

    /// Whether the two scopes can reach the same row through incompatible access.
    ///
    /// This is the only conflict question worth asking about two separate queries, and both
    /// halves carry weight: `collide_with` alone rejects a reader and a writer of one component
    /// even when no archetype can feed both, while disjointness alone says nothing about what
    /// happens once they do coincide.
    pub fn conflicts_with(&self, other: &AccessScope) -> bool
    {
        !self.matches_archetypes_disjoint_from(other) && self.collide_with(other)
    }

    /// One side writing what the other reads or writes. Shared reads are fine.
    ///
    /// Component-level only - it answers "is this pair of accesses incompatible", not "can
    /// these two ever meet on a row". Private because every caller outside a single query wants
    /// [`Self::conflicts_with`] instead.
    fn collide_with(&self, other: &AccessScope) -> bool
    {
        self.read.intersects(&other.write) // read while other write
        || self.write.intersects(&other.read) // write while other read
        || self.write.intersects(&other.write) // both write on the same
    }

    /// Whether the two select archetype sets that cannot overlap.
    ///
    /// A scope matches every archetype carrying all of its `read`/`write` and none of its
    /// `exclude`, so the matched sets are upward-closed: they would always meet at the union of
    /// both requirements, and `exclude` is the only thing able to keep them apart. That union
    /// archetype exists exactly when neither side excludes what the other requires - hence the
    /// check, in both directions.
    fn matches_archetypes_disjoint_from(&self, other: &AccessScope) -> bool
    {
        self.exclude.intersects(&other.read)
            || self.exclude.intersects(&other.write)
            || other.exclude.intersects(&self.read)
            || other.exclude.intersects(&self.write)
    }
}

/// Every access one system performs, one entry per query parameter.
///
/// A list rather than a merged [`AccessScope`], because unioning two parameters drops which
/// `exclude` belonged to which `read`/`write`. Merge `Query<(&mut Transform, With<Player>)>`
/// with `Query<(&Transform, Without<Player>)>` and the result claims to both require and
/// exclude `Player`; every disjointness answer read off that triple afterwards is noise, and it
/// errs towards handing parallelism to systems that really do share rows.
#[derive(Default, Clone)]
pub struct AccessScopes
{
    scopes: Vec<AccessScope>,
}
impl AccessScopes
{
    /// Registers one parameter, rejecting it when it conflicts with one already registered.
    ///
    /// A system's parameters are all initialised before its body runs, so every accessor is
    /// alive at once. A conflict here is aliasing rather than a scheduling question, and it
    /// holds on a single thread just as much as on many.
    pub fn push(&mut self, scope: AccessScope) -> Result<(), XynokEcsError>
    {
        if self.scopes.iter().any(|registered| registered.conflicts_with(&scope))
        {
            return Err(XynokEcsError::SystemAccessScopeConflict);
        }
        self.scopes.push(scope);
        Ok(())
    }

    /// Whether two systems may run at the same time: no parameter of one may conflict with any
    /// parameter of the other
    pub fn can_parallel_with(&self, other: &Self) -> bool
    {
        !self.scopes.iter().any(|mine| other.scopes.iter().any(|theirs| mine.conflicts_with(theirs)))
    }

    pub fn is_read_only(&self) -> bool
    {
        self.scopes.iter().all(AccessScope::is_read_only)
    }

    pub fn iter(&self) -> impl Iterator<Item = &AccessScope>
    {
        self.scopes.iter()
    }

    pub fn len(&self) -> usize
    {
        self.scopes.len()
    }

    pub fn is_empty(&self) -> bool
    {
        self.scopes.is_empty()
    }
}

#[cfg(test)]
mod test
{
    use super::{AccessScope, AccessScopes};
    use crate::collection::component_bit_set::ComponentBitSet;

    const TRANSFORM: usize = 0;
    const PLAYER: usize = 1;

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

    /// `Query<(&mut Transform, With<Player>)>` next to `Query<(&Transform, Without<Player>)>`:
    /// the pair the flat-triple model used to reject, and the reason the list exists
    fn moves_players() -> AccessScope
    {
        scope(&[PLAYER], &[TRANSFORM], &[])
    }
    fn reads_non_players() -> AccessScope
    {
        scope(&[TRANSFORM], &[], &[PLAYER])
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

    /// Exclusion buys nothing inside one query: its elements share a row by construction, so
    /// `Query<(&Transform, &mut Transform, Without<Player>)>` still aliases
    #[test]
    fn extend_ignores_exclusion_because_one_query_is_one_row()
    {
        let mut a = reads_non_players();
        assert!(a.extend(moves_players()).is_err(), "the same row would yield &Transform and &mut Transform");
    }

    #[test]
    fn no_conflict_when_nothing_is_shared()
    {
        assert!(!scope(&[0], &[1], &[]).conflicts_with(&scope(&[2], &[3], &[])));
        assert!(!scope(&[], &[], &[]).conflicts_with(&scope(&[], &[], &[])), "two scopes touching nothing");
    }

    #[test]
    fn no_conflict_when_both_only_read()
    {
        assert!(!scope(&[0, 1], &[], &[]).conflicts_with(&scope(&[1, 2], &[], &[])));
    }

    #[test]
    fn conflict_when_one_writes_what_the_other_touches()
    {
        assert!(scope(&[0], &[], &[]).conflicts_with(&scope(&[], &[0], &[])), "read vs write");
        assert!(scope(&[], &[0], &[]).conflicts_with(&scope(&[0], &[], &[])), "write vs read");
        assert!(scope(&[], &[0], &[]).conflicts_with(&scope(&[], &[0], &[])), "write vs write");
    }

    /// Both write component 0, but one only matches archetypes carrying component 1 while the
    /// other only matches those without it
    #[test]
    fn no_conflict_when_exclusion_makes_the_archetype_sets_disjoint()
    {
        let without_1 = scope(&[], &[0], &[1]);
        let with_1 = scope(&[1], &[0], &[]);

        assert!(!without_1.conflicts_with(&with_1), "no archetype can match both");
        assert!(!with_1.conflicts_with(&without_1), "and the check is symmetric");
    }

    #[test]
    fn exclusion_of_an_untouched_component_does_not_excuse_a_conflict()
    {
        // 9 is excluded by one side but neither read nor written by the other, so it says
        // nothing about whether their archetype sets overlap
        assert!(scope(&[], &[0], &[9]).conflicts_with(&scope(&[], &[0], &[])));
    }

    #[test]
    fn push_rejects_two_parameters_that_alias()
    {
        // system_c: Query<(&Hp, &Mana)> beside Query<&mut Hp>
        let mut scopes = AccessScopes::default();
        scopes.push(scope(&[0, 1], &[], &[])).expect("the first parameter always fits");
        assert!(scopes.push(scope(&[], &[0], &[])).is_err(), "&Hp and &mut Hp would meet on the same row");
        assert_eq!(scopes.len(), 1, "a rejected parameter must not be recorded");
    }

    #[test]
    fn push_accepts_parameters_kept_apart_by_exclusion()
    {
        let mut scopes = AccessScopes::default();
        scopes.push(moves_players()).expect("the first parameter always fits");
        scopes.push(reads_non_players()).expect("no archetype carries and lacks Player at once");
        assert_eq!(scopes.len(), 2);
    }

    #[test]
    fn push_compares_against_every_earlier_parameter_not_just_the_last()
    {
        let mut scopes = AccessScopes::default();
        scopes.push(scope(&[], &[7], &[])).expect("first");
        scopes.push(scope(&[3], &[], &[])).expect("unrelated to the first");
        assert!(scopes.push(scope(&[7], &[], &[])).is_err(), "conflicts with the first, not the second");
    }

    #[test]
    fn systems_parallelise_when_no_pair_of_parameters_conflicts()
    {
        let mut reader = AccessScopes::default();
        reader.push(scope(&[0], &[], &[])).unwrap();
        reader.push(scope(&[1], &[], &[])).unwrap();

        let mut writer = AccessScopes::default();
        writer.push(scope(&[], &[2], &[])).unwrap();

        assert!(reader.can_parallel_with(&writer));
        assert!(writer.can_parallel_with(&reader));
    }

    #[test]
    fn systems_do_not_parallelise_when_any_pair_conflicts()
    {
        let mut reader = AccessScopes::default();
        reader.push(scope(&[0], &[], &[])).unwrap();
        reader.push(scope(&[1], &[], &[])).unwrap();

        let mut writer = AccessScopes::default();
        writer.push(scope(&[], &[1], &[])).unwrap();

        assert!(!reader.can_parallel_with(&writer), "the second parameter collides with the writer");
        assert!(!writer.can_parallel_with(&reader), "and the check is symmetric");
    }

    /// The regression the list exists for. Merged into one triple these two parameters give
    /// `read {Transform, Player}, write {Transform}, exclude {Player}` - a scope that both
    /// requires and excludes `Player`, and that would then read as disjoint from any system
    /// touching `Player`, granting parallelism over rows they really do share.
    #[test]
    fn a_system_holding_both_parameters_still_blocks_a_player_writer()
    {
        let mut both = AccessScopes::default();
        both.push(moves_players()).unwrap();
        both.push(reads_non_players()).unwrap();

        let mut player_writer = AccessScopes::default();
        player_writer.push(scope(&[], &[PLAYER], &[])).unwrap();

        assert!(!both.can_parallel_with(&player_writer), "the first parameter reads Player");
    }

    #[test]
    fn read_only_needs_every_parameter_to_be_read_only()
    {
        let mut scopes = AccessScopes::default();
        assert!(scopes.is_read_only(), "a system with no queries writes nothing");

        scopes.push(scope(&[0], &[], &[])).unwrap();
        assert!(scopes.is_read_only());

        scopes.push(scope(&[], &[5], &[])).unwrap();
        assert!(!scopes.is_read_only());
    }
}
