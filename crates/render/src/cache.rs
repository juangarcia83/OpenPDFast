// SPDX-License-Identifier: AGPL-3.0-or-later

//! LRU cache bounded by an approximate memory cost.

use std::hash::Hash;
use std::sync::Arc;

use lru::LruCache;

pub struct CostLru<K: Hash + Eq, V> {
    map: LruCache<K, (Arc<V>, usize)>,
    cost: usize,
    budget: usize,
}

impl<K: Hash + Eq, V> CostLru<K, V> {
    pub fn new(budget: usize) -> Self {
        Self {
            map: LruCache::unbounded(),
            cost: 0,
            budget,
        }
    }

    pub fn get(&mut self, key: &K) -> Option<Arc<V>> {
        self.map.get(key).map(|(v, _)| Arc::clone(v))
    }

    pub fn contains(&self, key: &K) -> bool {
        self.map.contains(key)
    }

    /// Inserts and evicts least-recently-used entries until within budget.
    /// An entry larger than the whole budget is not kept.
    pub fn put(&mut self, key: K, value: Arc<V>, cost: usize) {
        if let Some((_, old)) = self.map.put(key, (value, cost)) {
            self.cost -= old;
        }
        self.cost += cost;
        while self.cost > self.budget {
            match self.map.pop_lru() {
                Some((_, (_, c))) => self.cost -= c,
                None => break,
            }
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    #[cfg(test)]
    fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    pub fn cost(&self) -> usize {
        self.cost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used_within_budget() {
        let mut c = CostLru::new(10);
        c.put(1, Arc::new("a"), 4);
        c.put(2, Arc::new("b"), 4);
        assert!(c.get(&1).is_some()); // 1 becomes most recent
        c.put(3, Arc::new("c"), 4); // evicts 2
        assert!(c.contains(&1) && c.contains(&3) && !c.contains(&2));
        assert_eq!(c.cost(), 8);
        c.put(4, Arc::new("huge"), 50);
        assert!(c.is_empty());
        assert_eq!(c.cost(), 0);
    }

    #[test]
    fn replacing_a_key_updates_cost() {
        let mut c = CostLru::new(10);
        c.put(1, Arc::new(()), 6);
        c.put(1, Arc::new(()), 3);
        assert_eq!((c.len(), c.cost()), (1, 3));
    }
}
