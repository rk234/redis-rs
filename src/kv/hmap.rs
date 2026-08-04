use super::htable::HTab;
use std::{mem, ptr};

const MAX_LOAD_FACTOR: usize = 8;
const REHASH_CHUNK: usize = 128;

pub struct HMap<T: Clone> {
    newer: HTab<T>,
    older: Option<HTab<T>>,
    migrate_pos: usize,
}

impl<T: Clone> HMap<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        HMap {
            newer: HTab::with_capacity(capacity),
            older: None,
            migrate_pos: 0,
        }
    }

    pub fn insert(&mut self, hash: usize, val: T) {
        self.newer.insert(hash, val);

        if self.newer.size > (self.newer.mask + 1) * MAX_LOAD_FACTOR && self.older.is_none() {
            self.trigger_rehashing();
        }

        if self.older.is_some() {
            self.rehash();
        }
    }

    pub fn lookup(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<&T> {
        match self.newer.lookup(hash, &eq) {
            Some(v) => Some(v),
            None => self.older.as_mut()?.lookup(hash, eq),
        }
    }

    pub fn lookup_mut(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<&mut T> {
        match self.newer.lookup_mut(hash, &eq) {
            Some(v) => Some(v),
            None => self.older.as_mut()?.lookup_mut(hash, eq),
        }
    }

    pub fn remove(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<T> {
        match self.newer.remove(hash, &eq) {
            None => self.older.as_mut()?.remove(hash, eq),
            v => v,
        }
    }

    fn trigger_rehashing(&mut self) {
        let new_cap = (self.newer.mask + 1) << 1;
        let old = mem::replace(&mut self.newer, HTab::with_capacity(new_cap));
        self.older = Some(old);
        self.migrate_pos = 0;
    }

    fn rehash(&mut self) {
        let mut moved = 0;
        if let Some(older) = self.older.as_mut() {
            while moved < REHASH_CHUNK {
                if self.migrate_pos > older.mask {
                    self.older = None;
                    return;
                }

                let head = unsafe { older.tab.add(self.migrate_pos) };
                let slot = unsafe { *head };
                if slot.is_null() {
                    self.migrate_pos += 1;
                    continue;
                }

                unsafe {
                    *head = (*slot).next;
                    self.newer.insert((*slot).hcode, (*slot).val.clone());
                    (*slot).next = ptr::null_mut();
                    drop(Box::from_raw(slot));
                }
                moved += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kv::pair::KVPair;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn hash_key(key: &str) -> usize {
        let mut h = DefaultHasher::new();
        key.hash(&mut h);
        h.finish() as usize
    }

    fn kv(key: &str, val: &str) -> KVPair {
        KVPair::new(key.to_string(), val.to_string())
    }

    fn eq_key(key: &str) -> impl Fn(&KVPair) -> bool + '_ {
        move |p| p.key == key
    }

    #[test]
    fn hmap_lookup_empty() {
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        assert!(map.lookup(hash_key("foo"), eq_key("foo")).is_none());
    }

    #[test]
    fn hmap_insert_and_lookup() {
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        let h = hash_key("city");
        map.insert(h, kv("city", "tokyo"));
        assert_eq!(
            map.lookup(h, eq_key("city")).map(|p| p.val.as_str()),
            Some("tokyo")
        );
    }

    #[test]
    fn hmap_remove_returns_value() {
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        let h = hash_key("lang");
        map.insert(h, kv("lang", "rust"));
        assert_eq!(
            map.remove(h, eq_key("lang")).map(|p| p.val),
            Some("rust".to_string())
        );
    }

    #[test]
    fn hmap_remove_missing_returns_none() {
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        assert!(map.remove(hash_key("ghost"), eq_key("ghost")).is_none());
    }

    #[test]
    fn hmap_all_items_survive_rehash() {
        // capacity=4, MAX_LOAD_FACTOR=8 → rehash triggers at size > 32
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        for i in 0..40usize {
            let key = format!("key{i}");
            map.insert(hash_key(&key), kv(&key, &i.to_string()));
        }
        for i in 0..40usize {
            let key = format!("key{i}");
            assert!(
                map.lookup(hash_key(&key), eq_key(&key)).is_some(),
                "key{i} missing after rehash"
            );
        }
    }

    #[test]
    fn hmap_lookup_spans_older_and_newer() {
        // capacity=16, MAX_LOAD_FACTOR=8 → rehash triggers at size > 128.
        // After 129th insert: 129 items moved to older, rehash migrates 128 of them,
        // leaving 1 in older. All 129 must still be findable.
        let mut map: HMap<KVPair> = HMap::with_capacity(16);
        for i in 0..129usize {
            let key = format!("k{i}");
            map.insert(hash_key(&key), kv(&key, &i.to_string()));
        }
        for i in 0..129usize {
            let key = format!("k{i}");
            assert!(
                map.lookup(hash_key(&key), eq_key(&key)).is_some(),
                "k{i} not found while straddling older and newer"
            );
        }
    }

    #[test]
    fn hmap_remove_spans_older_and_newer() {
        let mut map: HMap<KVPair> = HMap::with_capacity(16);
        for i in 0..129usize {
            let key = format!("k{i}");
            map.insert(hash_key(&key), kv(&key, &i.to_string()));
        }
        for i in 0..129usize {
            let key = format!("k{i}");
            assert!(
                map.remove(hash_key(&key), eq_key(&key)).is_some(),
                "k{i} not removable while straddling older and newer"
            );
        }
        for i in 0..129usize {
            let key = format!("k{i}");
            assert!(
                map.lookup(hash_key(&key), eq_key(&key)).is_none(),
                "k{i} still present after remove"
            );
        }
    }

    #[test]
    fn hmap_no_duplicates_after_migration() {
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        for i in 0..40usize {
            let key = format!("item{i}");
            map.insert(hash_key(&key), kv(&key, &i.to_string()));
        }
        for i in 0..40usize {
            let key = format!("item{i}");
            let h = hash_key(&key);
            assert!(map.remove(h, eq_key(&key)).is_some(), "item{i} missing");
            assert!(map.remove(h, eq_key(&key)).is_none(), "item{i} found twice");
        }
    }
}
