pub struct ArchetypeIdentifySpec
{
    pub id: usize,
}
#[cfg(test)]
mod test
{
    // ------------------------------------------------------------------
    // Level 1 - derive: the Rust equivalent of a C# record's auto-generated
    // Equals/GetHashCode, or a manual `IEquatable<T>` + `GetHashCode` override.
    // `#[derive(PartialEq, Eq, Hash)]` compares/combines every field in
    // declaration order - no interface to implement, just three derives.
    // ------------------------------------------------------------------
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct Point
    {
        x: i32,
        y: i32,
    }

    #[test]
    fn level1_derive_as_hashmap_key()
    {
        use std::collections::HashMap;

        let mut grid: HashMap<Point, &'static str> = HashMap::new();
        grid.insert(Point { x: 0, y: 0 }, "origin");
        grid.insert(Point { x: 1, y: 2 }, "goblin");

        // A freshly-built Point with the same fields is a different value but an equal *key* -
        // same idea as overriding Equals/GetHashCode instead of relying on reference equality.
        assert_eq!(grid.get(&Point { x: 1, y: 2 }), Some(&"goblin"));
    }

    // ------------------------------------------------------------------
    // Level 2 - why Rust splits `PartialEq` from `Eq`. C# lets a `double`
    // sit in a Dictionary key without complaint, NaN included, which quietly
    // breaks lookups. Rust's `Eq` is a marker that promises reflexivity
    // (`x == x` for every value); `f64` can't promise that (`NaN != NaN`),
    // so `f64` implements `PartialEq` but never `Eq`. A struct holding an
    // `f64` can't derive `Eq`/`Hash` either - the compiler refuses it as a
    // HashMap key instead of letting it fail silently at runtime like C# does.
    // ------------------------------------------------------------------
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct MeshTransform
    {
        scale: f64, // PartialEq only - try adding `Eq` to the derive above and see it fail to compile
    }

    #[test]
    fn level2_partial_eq_vs_eq()
    {
        let a = MeshTransform { scale: 1.0 };
        let b = MeshTransform { scale: 1.0 };
        assert_eq!(a, b); // PartialEq is enough for plain `==`

        let nan = MeshTransform { scale: f64::NAN };
        assert_ne!(nan, nan); // reflexivity broken - exactly why Eq can't be derived here
    }

    // ------------------------------------------------------------------
    // Level 3 - hand-written `Hash` + `PartialEq`/`Eq`: the direct analog of
    // a manual C# `Equals`/`GetHashCode` override, for when identity should
    // ignore some fields (here, a cosmetic debug label).
    // Rule that must always hold: if `a == b` then `hash(a) == hash(b)`.
    // Get this wrong and HashMap silently drops or "loses" entries.
    // ------------------------------------------------------------------
    use std::hash::{Hash, Hasher};

    #[derive(Debug, Clone)]
    struct Prefab
    {
        id:    u32,
        label: String, // purely cosmetic, must not affect equality/hash
    }

    impl PartialEq for Prefab
    {
        fn eq(&self, other: &Self) -> bool
        {
            self.id == other.id
        }
    }
    impl Eq for Prefab {}

    impl Hash for Prefab
    {
        fn hash<H: Hasher>(&self, state: &mut H)
        {
            self.id.hash(state); // must mirror exactly the fields `eq` compares
        }
    }

    #[test]
    fn level3_hash_ignores_cosmetic_field()
    {
        use std::collections::hash_map::DefaultHasher;

        let a = Prefab {
            id:    7,
            label: "goblin".into(),
        };
        let b = Prefab {
            id:    7,
            label: "renamed in editor".into(),
        };
        assert_eq!(a, b); // equal despite different labels

        let hash_of = |p: &Prefab| {
            let mut h = DefaultHasher::new();
            p.hash(&mut h);
            h.finish()
        };
        assert_eq!(hash_of(&a), hash_of(&b));
    }

    // ------------------------------------------------------------------
    // Level 4 - order-independent hashing for a *set* of component types.
    // This is xynok_ecs's actual problem: (Hp, Mana, Mesh) and (Mesh, Hp, Mana)
    // must be the same archetype. `Vec<TypeId>`'s derived Hash hashes elements
    // in order, so two vecs holding the same elements in different order hash
    // differently - a bug for something meant to behave like an unordered set.
    // `utils::normalize_set` already fixes this the same way: sort + dedup
    // *before* hashing, so equal sets always produce an identical byte sequence.
    // ------------------------------------------------------------------
    #[test]
    fn level4_order_independent_set_hash()
    {
        use std::any::TypeId;
        use std::collections::hash_map::DefaultHasher;

        struct Hp;
        struct Mana;
        struct Mesh;

        let hash_of = |types: &[TypeId]| {
            let mut h = DefaultHasher::new();
            types.hash(&mut h);
            h.finish()
        };

        let a_unsorted = vec![TypeId::of::<Hp>(), TypeId::of::<Mana>(), TypeId::of::<Mesh>()];
        let b_unsorted = vec![TypeId::of::<Mesh>(), TypeId::of::<Hp>(), TypeId::of::<Mana>()];
        assert_ne!(hash_of(&a_unsorted), hash_of(&b_unsorted)); // same set, different hash - the bug

        let mut a_sorted = a_unsorted.clone();
        let mut b_sorted = b_unsorted.clone();
        a_sorted.sort();
        b_sorted.sort();
        assert_eq!(hash_of(&a_sorted), hash_of(&b_sorted)); // canonical order first - the fix
    }

    // ------------------------------------------------------------------
    // Level 5 (production) - a composite archetype key: structural type set
    // + an optional shared-component value id, usable directly as a HashMap
    // key. This is the shape discussed for extending `ArchetypeIdentifySpec`/
    // `World::archetypes` to support Unity/Mass-style value-based chunk
    // filtering (`SetSharedComponentFilter`) without needing a new Rust type
    // per shared value.
    // ------------------------------------------------------------------
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    struct SharedValueId(u64);

    impl SharedValueId
    {
        /// Interning-style id: in production this id must come from a
        /// `HashMap<Value, SharedValueId>` side table (lookup-or-insert), so
        /// equal values always resolve to the same id. Hashing the value
        /// directly (as done here) is only safe when the value's own `Hash`
        /// impl already guarantees no practical collisions for your input
        /// space - otherwise two distinct values can collide into one id.
        fn from_hash_of<T: Hash>(value: &T) -> Self
        {
            use std::collections::hash_map::DefaultHasher;
            let mut hasher = DefaultHasher::new();
            value.hash(&mut hasher);
            Self(hasher.finish())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct ArchetypeKey
    {
        type_set:   Vec<std::any::TypeId>, // must already be sorted (see level 4) before landing here
        shared_val: Option<SharedValueId>, // None = archetype has no shared component
    }

    #[test]
    fn level5_composite_archetype_key_as_hashmap_key()
    {
        use std::any::TypeId;
        use std::collections::HashMap;

        struct Hp;
        struct Mana;
        struct Mesh;

        let mut type_set = vec![TypeId::of::<Hp>(), TypeId::of::<Mana>(), TypeId::of::<Mesh>()];
        type_set.sort();

        let key_monkey = ArchetypeKey {
            type_set:   type_set.clone(),
            shared_val: Some(SharedValueId::from_hash_of(&"monkey.mesh")),
        };
        let key_same_monkey = ArchetypeKey {
            type_set:   type_set.clone(),
            shared_val: Some(SharedValueId::from_hash_of(&"monkey.mesh")),
        };
        let key_orc = ArchetypeKey {
            type_set:   type_set.clone(),
            shared_val: Some(SharedValueId::from_hash_of(&"orc.mesh")),
        };

        let mut archetypes: HashMap<ArchetypeKey, &'static str> = HashMap::new();
        archetypes.insert(key_monkey.clone(), "monkey archetype");
        archetypes.insert(key_orc.clone(), "orc archetype");

        // Same shared value -> same key -> hits the existing archetype, no new type needed.
        assert_eq!(archetypes.get(&key_same_monkey), Some(&"monkey archetype"));
        // Different shared value -> genuinely different archetype, chunk-filterable like Unity's
        // SetSharedComponentFilter, without ever touching `TypeId::of::<T>()`.
        assert_ne!(key_monkey, key_orc);
    }
}
