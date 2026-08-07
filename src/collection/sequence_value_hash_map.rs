#![allow(unused)]
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::hash::Hash;
use std::slice::{Iter, IterMut};

/// An **append-only** map that keeps its values in a dense `Vec`, so every key also gets a
/// `usize` index that stays valid for the lifetime of the map.
///
/// The intended access pattern is to pay for the hash lookup once (at registration time) and
/// then hold on to the index, reaching values through [`Self::value_at`] on the hot path.
///
/// `keys` runs in lockstep with `values`, so iteration is dense and needs no hashing, and
/// [`Self::keys`] and [`Self::values`] always agree on ordering.
///
/// There is deliberately no `remove`: dropping an entry would either invalidate the indices
/// handed out earlier (`swap_remove`) or leave holes that every iterator would have to skip.
/// The `World` registries this backs only ever grow.
pub struct SequenceValueHashMap<K, V>
{
    indices: HashMap<K, usize>,
    keys:    Vec<K>,
    values:  Vec<V>,
}

impl<K, V> SequenceValueHashMap<K, V>
{
    pub fn new() -> Self
    {
        Self {
            indices: HashMap::new(),
            keys:    Vec::new(),
            values:  Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self
    {
        Self {
            indices: HashMap::with_capacity(capacity),
            keys:    Vec::with_capacity(capacity),
            values:  Vec::with_capacity(capacity),
        }
    }

    #[inline]
    pub fn len(&self) -> usize
    {
        self.values.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.values.is_empty()
    }

    #[inline]
    pub fn value_at(&self, idx: usize) -> Option<&V>
    {
        self.values.get(idx)
    }

    #[inline]
    pub fn value_mut_at(&mut self, idx: usize) -> Option<&mut V>
    {
        self.values.get_mut(idx)
    }

    #[inline]
    pub fn key_at(&self, idx: usize) -> Option<&K>
    {
        self.keys.get(idx)
    }

    /// Insertion order, matching [`Self::values`] index for index
    #[inline]
    pub fn keys(&self) -> Iter<'_, K>
    {
        self.keys.iter()
    }

    /// Insertion order, so position `i` is the value reached by `value_at(i)`
    #[inline]
    pub fn values(&self) -> Iter<'_, V>
    {
        self.values.iter()
    }

    #[inline]
    pub fn values_mut(&mut self) -> IterMut<'_, V>
    {
        self.values.iter_mut()
    }

    /// The dense storage itself, so callers can reach for slice APIs the map does not
    /// wrap - notably `get_disjoint_mut` to borrow several entries at once
    #[inline]
    pub fn values_mut_slice(&mut self) -> &mut [V]
    {
        &mut self.values
    }

    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> + '_
    {
        self.keys.iter().zip(self.values.iter())
    }

    #[inline]
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> + '_
    {
        self.keys.iter().zip(self.values.iter_mut())
    }
}

impl<K, V> Default for SequenceValueHashMap<K, V>
{
    fn default() -> Self
    {
        Self::new()
    }
}

impl<K: Eq + Hash, V> SequenceValueHashMap<K, V>
{
    /// The one lookup worth caching: resolve a key to its index once, then stay on
    /// [`Self::value_at`] afterwards
    #[inline]
    pub fn index_of(&self, k: &K) -> Option<usize>
    {
        self.indices.get(k).copied()
    }

    #[inline]
    pub fn contains_key(&self, k: &K) -> bool
    {
        self.indices.contains_key(k)
    }

    #[inline]
    pub fn get(&self, k: &K) -> Option<&V>
    {
        self.indices.get(k).map(|&idx| &self.values[idx])
    }

    #[inline]
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V>
    {
        self.indices.get(k).map(|&idx| &mut self.values[idx])
    }
}

impl<K: Eq + Hash + Clone, V> SequenceValueHashMap<K, V>
{
    /// Overwrites the value if `k` is already present, keeping its existing index.
    /// Returns the index of the value either way.
    pub fn insert(&mut self, k: K, val: V) -> usize
    {
        match self.indices.entry(k)
        {
            Entry::Occupied(e) =>
            {
                let idx = *e.get();
                self.values[idx] = val;
                idx
            }
            Entry::Vacant(e) =>
            {
                let idx = self.values.len();
                self.keys.push(e.key().clone());
                e.insert(idx);
                self.values.push(val);
                idx
            }
        }
    }

    /// Returns the index of `k`, building the value only when it is missing
    pub fn get_or_insert_with(&mut self, k: K, f: impl FnOnce() -> V) -> usize
    {
        match self.indices.entry(k)
        {
            Entry::Occupied(e) => *e.get(),
            Entry::Vacant(e) =>
            {
                let idx = self.values.len();
                self.keys.push(e.key().clone());
                e.insert(idx);
                self.values.push(f());
                idx
            }
        }
    }
}

#[cfg(test)]
mod test
{
    use super::SequenceValueHashMap;

    fn sample() -> SequenceValueHashMap<&'static str, u32>
    {
        let mut map = SequenceValueHashMap::new();
        map.insert("a", 10);
        map.insert("b", 20);
        map.insert("c", 30);
        map
    }

    #[test]
    fn insert_returns_dense_indices()
    {
        let mut map: SequenceValueHashMap<&str, u32> = SequenceValueHashMap::new();
        assert!(map.insert("a", 10) == 0);
        assert!(map.insert("b", 20) == 1);
        assert!(map.insert("c", 30) == 2);
        assert!(map.len() == 3);
    }

    #[test]
    fn reinsert_overwrites_and_keeps_index()
    {
        let mut map = sample();
        assert!(map.insert("b", 99) == 1);
        assert!(map.len() == 3);
        assert!(map.get(&"b") == Some(&99));
        assert!(map.value_at(1) == Some(&99));
    }

    #[test]
    fn index_survives_later_inserts()
    {
        let mut map = sample();
        let idx = map.index_of(&"a").unwrap();
        for i in 0..64
        {
            map.insert(Box::leak(format!("k{i}").into_boxed_str()), i);
        }
        assert!(map.index_of(&"a") == Some(idx));
        assert!(map.value_at(idx) == Some(&10));
    }

    #[test]
    fn get_or_insert_with_runs_once()
    {
        let mut map: SequenceValueHashMap<&str, u32> = SequenceValueHashMap::new();
        let mut calls = 0;
        let first = map.get_or_insert_with("a", || {
            calls += 1;
            7
        });
        let second = map.get_or_insert_with("a", || {
            calls += 1;
            9
        });
        assert!(first == second);
        assert!(calls == 1);
        assert!(map.get(&"a") == Some(&7));
    }

    #[test]
    fn keys_and_values_share_insertion_order()
    {
        let map = sample();
        let keys: Vec<_> = map.keys().copied().collect();
        let values: Vec<_> = map.values().copied().collect();
        assert!(keys == ["a", "b", "c"]);
        assert!(values == [10, 20, 30]);

        for (i, (k, v)) in map.iter().enumerate()
        {
            assert!(map.key_at(i) == Some(k));
            assert!(map.value_at(i) == Some(v));
        }
    }

    #[test]
    fn iter_mut_pairs_the_right_value()
    {
        let mut map = sample();
        for (k, v) in map.iter_mut()
        {
            if *k == "b"
            {
                *v = 21;
            }
        }
        assert!(map.get(&"b") == Some(&21));
        assert!(map.get(&"a") == Some(&10));
    }

    #[test]
    fn missing_key_and_out_of_range_index()
    {
        let map = sample();
        assert!(map.get(&"zzz").is_none());
        assert!(map.index_of(&"zzz").is_none());
        assert!(!map.contains_key(&"zzz"));
        assert!(map.value_at(3).is_none());
        assert!(map.key_at(3).is_none());
    }
}
