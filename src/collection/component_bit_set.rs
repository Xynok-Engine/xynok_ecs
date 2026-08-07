#![allow(unused)]
/// How many component ids fit in one word of the set
pub const BITS_PER_WORD: usize = 64;

#[derive(Default, Clone, Debug)]
pub struct ComponentBitSet
{
    words: Vec<u64>,
}

impl ComponentBitSet
{
    pub fn with_capacity_for(component_count: usize) -> Self
    {
        Self {
            words: Vec::with_capacity(component_count.div_ceil(BITS_PER_WORD)),
        }
    }

    #[inline]
    pub fn clear(&mut self)
    {
        self.words.clear();
    }

    #[inline]
    pub fn is_empty(&self) -> bool
    {
        self.words.iter().all(|w| *w == 0)
    }

    #[inline]
    pub fn insert(&mut self, id: usize)
    {
        let (word, bit) = (id / BITS_PER_WORD, id % BITS_PER_WORD);
        if self.words.len() <= word
        {
            self.words.resize(word + 1, 0);
        }
        self.words[word] |= 1u64 << bit;
    }

    #[inline]
    pub fn remove(&mut self, id: usize)
    {
        let (word, bit) = (id / BITS_PER_WORD, id % BITS_PER_WORD);
        if let Some(w) = self.words.get_mut(word)
        {
            *w &= !(1u64 << bit);
        }
    }

    #[inline]
    pub fn contains(&self, id: usize) -> bool
    {
        let (word, bit) = (id / BITS_PER_WORD, id % BITS_PER_WORD);
        self.words.get(word).is_some_and(|w| w & (1u64 << bit) != 0)
    }

    /// No bit in common. `zip` stops at the shorter buffer, which is correct because the extra words
    /// of the longer one would only ever be `AND`ed against zero.
    #[inline]
    pub fn is_disjoint(&self, other: &Self) -> bool
    {
        self.words.iter().zip(other.words.iter()).all(|(a, b)| a & b == 0)
    }

    #[inline]
    pub fn intersects(&self, other: &Self) -> bool
    {
        !self.is_disjoint(other)
    }

    /// Every bit set in `other` is also set here. An empty `other` is trivially contained,
    /// which is why callers must never let an unresolved component silently produce one.
    pub fn contains_all(&self, other: &Self) -> bool
    {
        for (idx, word) in other.words.iter().enumerate()
        {
            let mine = self.words.get(idx).copied().unwrap_or(0);
            if word & !mine != 0
            {
                return false;
            }
        }
        true
    }

    /// Ids of the set bits, ascending
    #[inline]
    pub fn iter(&self) -> ComponentBitSetIter<'_>
    {
        ComponentBitSetIter {
            words:    &self.words,
            word_idx: 0,
            current:  self.words.first().copied().unwrap_or(0),
        }
    }

    pub fn copy_from(&mut self, src: &Self)
    {
        self.words.clear();
        self.words.extend_from_slice(&src.words);
    }

    pub fn union_with(&mut self, other: &Self)
    {
        if self.words.len() < other.words.len()
        {
            self.words.resize(other.words.len(), 0);
        }
        for (dst, src) in self.words.iter_mut().zip(other.words.iter())
        {
            *dst |= *src;
        }
    }
}

pub struct ComponentBitSetIter<'a>
{
    words:    &'a [u64],
    word_idx: usize,
    current:  u64,
}

impl Iterator for ComponentBitSetIter<'_>
{
    type Item = usize;

    fn next(&mut self) -> Option<usize>
    {
        loop
        {
            if self.current != 0
            {
                let bit = self.current.trailing_zeros() as usize;
                // Clear the lowest set bit, so each id is yielded exactly once
                self.current &= self.current - 1;
                return Some(self.word_idx * BITS_PER_WORD + bit);
            }
            self.word_idx += 1;
            self.current = *self.words.get(self.word_idx)?;
        }
    }
}

#[cfg(test)]
mod test
{
    use super::{ComponentBitSet, BITS_PER_WORD};

    fn set_of(ids: &[usize]) -> ComponentBitSet
    {
        let mut s = ComponentBitSet::default();
        for id in ids
        {
            s.insert(*id);
        }
        s
    }

    #[test]
    fn iter_yields_inserted_ids_ascending()
    {
        let s = set_of(&[70, 0, 3, BITS_PER_WORD, 129]);
        let got: Vec<_> = s.iter().collect();
        assert!(got == [0, 3, BITS_PER_WORD, 70, 129], "got {got:?}");
    }

    #[test]
    fn iter_is_empty_for_empty_set()
    {
        assert!(ComponentBitSet::default().iter().next().is_none());
        assert!(set_of(&[5]).iter().count() == 1);
    }

    #[test]
    fn iter_skips_removed_ids()
    {
        let mut s = set_of(&[1, 2, 3]);
        s.remove(2);
        let got: Vec<_> = s.iter().collect();
        assert!(got == [1, 3], "got {got:?}");
    }

    #[test]
    fn contains_all_ignores_trailing_empty_words()
    {
        let a = set_of(&[1, 2, 200]);
        let b = set_of(&[1, 2]);
        assert!(a.contains_all(&b));
        assert!(!b.contains_all(&a), "b lacks id 200");
    }

    #[test]
    fn contains_all_of_empty_is_true()
    {
        assert!(set_of(&[1]).contains_all(&ComponentBitSet::default()));
        assert!(ComponentBitSet::default().contains_all(&ComponentBitSet::default()));
    }

    #[test]
    fn contains_all_of_self_is_true()
    {
        let a = set_of(&[0, 63, 64, 300]);
        assert!(a.contains_all(&a));
    }
}
