use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    hash::Hash,
    ptr,
};

const MAX_LOAD_FACTOR: usize = 8;
const REHASH_CHUNK: usize = 128;

pub struct KVPair {
    pub key: String,
    pub val: String,
}

impl KVPair {
    pub fn new(key: String, val: String) -> Self {
        KVPair { key, val }
    }
}

impl Hash for KVPair {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.key.hash(state);
    }
}

impl Clone for KVPair {
    fn clone(&self) -> Self {
        Self {
            key: self.key.clone(),
            val: self.val.clone(),
        }
    }
}

pub struct HNode<T> {
    next: *mut HNode<T>,
    hcode: usize,
    val: T,
}

impl<T> Drop for HNode<T> {
    fn drop(&mut self) {
        if !self.next.is_null() {
            unsafe { drop(Box::from_raw(self.next)) }
        }
    }
}

pub struct HTab<T> {
    tab: *mut *mut HNode<T>,
    mask: usize,
    size: usize,
}

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

    pub fn remove(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<T> {
        match self.newer.remove(hash, &eq) {
            None => self.older.as_mut()?.remove(hash, eq),
            v => v,
        }
    }

    fn trigger_rehashing(&mut self) {
        let new_cap = (self.newer.mask + 1) << 1;
        let old = std::mem::replace(&mut self.newer, HTab::with_capacity(new_cap));
        self.older = Some(old);
        self.migrate_pos = 0;
    }

    fn rehash(&mut self) {
        let mut moved = 0;
        if let Some(older) = self.older.as_mut() {
            while moved < REHASH_CHUNK {
                if self.migrate_pos > older.mask {
                    // migration done
                    self.older = None;
                    return;
                }

                let head = unsafe { older.tab.add(self.migrate_pos) };
                let slot = unsafe { *head };
                if slot.is_null() {
                    // empty slot, can move on to the next slot
                    self.migrate_pos += 1;
                    continue;
                }

                unsafe {
                    *head = (*slot).next;
                    self.newer.insert((*slot).hcode, (*slot).val.clone());
                    (*slot).next = ptr::null_mut(); // prevent HNode::drop cascading into live chain
                    drop(Box::from_raw(slot));
                }
                moved += 1;
            }
        }
    }
}

impl<T> HNode<T> {
    fn new(val: T, hcode: usize) -> HNode<T> {
        HNode {
            next: ptr::null_mut(),
            hcode: hcode,
            val: val,
        }
    }
}

impl<T: Clone> HTab<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        let layout = Layout::array::<*mut HNode<T>>(capacity).unwrap();
        let slots = unsafe { alloc_zeroed(layout) as *mut *mut HNode<T> };
        HTab {
            mask: capacity - 1, // mod trick: n % capacity => n & (capacity - 1)
            tab: slots,
            size: 0,
        }
    }

    pub fn insert(&mut self, hash: usize, val: T) {
        let node = Box::into_raw(Box::new(HNode::new(val, hash)));
        let pos = hash & self.mask;

        unsafe {
            (*node).next = *self.tab.add(pos);
            self.tab.add(pos).replace(node);
            self.size += 1;
        }
    }

    pub fn lookup(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<&T> {
        let pos = hash & self.mask;

        let mut node = unsafe { (*self.tab.add(pos)).as_ref() };
        while let Some(n) = node {
            let val = &n.val;
            if n.hcode == hash && eq(val) {
                return Some(&n.val);
            }

            node = unsafe { n.next.as_ref() };
        }

        return None;
    }

    pub fn remove(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<T> {
        let pos = hash & self.mask;

        unsafe {
            let mut prev: *mut *mut HNode<T> = self.tab.add(pos);
            let mut node = *prev;
            while !node.is_null() {
                if (*node).hcode == hash && eq(&(*node).val) {
                    *prev = (*node).next;
                    (*node).next = ptr::null_mut(); // prevent HNode::drop cascading into live chain
                    self.size -= 1;
                    return Some(Box::from_raw(node).val.clone());
                }
                prev = &raw mut (*node).next;
                node = (*node).next;
            }
        }
        return None;
    }
}

impl<T> Drop for HTab<T> {
    fn drop(&mut self) {
        let layout = Layout::array::<*mut HNode<T>>(self.mask + 1).unwrap();

        for i in 0..(self.mask + 1) {
            let head = unsafe { *self.tab.add(i) };
            if !head.is_null() {
                unsafe {
                    drop(Box::from_raw(head));
                }
            }
        }
        unsafe {
            dealloc(self.tab as *mut u8, layout);
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn lookup_empty_table() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        assert!(tab.lookup(hash_key("foo"), eq_key("foo")).is_none());
    }

    #[test]
    fn insert_and_lookup() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        let h = hash_key("name");
        tab.insert(h, kv("name", "redis"));
        assert_eq!(
            tab.lookup(h, eq_key("name")).map(|p| p.val.as_str()),
            Some("redis")
        );
    }

    #[test]
    fn lookup_wrong_key_same_hcode() {
        // Same hcode, eq returns false — simulates true hash collision
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        tab.insert(42, kv("foo", "bar"));
        assert!(tab.lookup(42, eq_key("baz")).is_none());
    }

    #[test]
    fn remove_returns_value() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        let h = hash_key("host");
        tab.insert(h, kv("host", "localhost"));
        let removed = tab.remove(h, eq_key("host"));
        assert_eq!(removed.map(|p| p.val), Some("localhost".to_string()));
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        assert!(tab.remove(hash_key("ghost"), eq_key("ghost")).is_none());
    }

    #[test]
    fn remove_makes_key_unreachable() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        let h = hash_key("port");
        tab.insert(h, kv("port", "6379"));
        tab.remove(h, eq_key("port"));
        assert!(tab.lookup(h, eq_key("port")).is_none());
    }

    #[test]
    fn collision_chain_all_reachable() {
        // hcodes 0, 8, 16 all map to slot 0 with mask=7
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        tab.insert(0, kv("a", "1"));
        tab.insert(8, kv("b", "2"));
        tab.insert(16, kv("c", "3"));
        assert!(tab.lookup(0, eq_key("a")).is_some());
        assert!(tab.lookup(8, eq_key("b")).is_some());
        assert!(tab.lookup(16, eq_key("c")).is_some());
    }

    #[test]
    fn remove_head_of_chain() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        tab.insert(0, kv("a", "1"));
        tab.insert(8, kv("b", "2")); // prepended, so b is the head
        tab.remove(8, eq_key("b"));
        assert!(tab.lookup(8, eq_key("b")).is_none());
        assert!(tab.lookup(0, eq_key("a")).is_some());
    }

    #[test]
    fn remove_middle_of_chain() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        tab.insert(0, kv("a", "1"));
        tab.insert(8, kv("b", "2"));
        tab.insert(16, kv("c", "3"));
        tab.remove(8, eq_key("b"));
        assert!(tab.lookup(0, eq_key("a")).is_some());
        assert!(tab.lookup(8, eq_key("b")).is_none());
        assert!(tab.lookup(16, eq_key("c")).is_some());
    }

    #[test]
    fn multiple_keys_same_hcode() {
        // True hash collision: same hcode, eq distinguishes them
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        tab.insert(99, kv("x", "val_x"));
        tab.insert(99, kv("y", "val_y"));
        assert_eq!(
            tab.lookup(99, eq_key("x")).map(|p| p.val.as_str()),
            Some("val_x")
        );
        assert_eq!(
            tab.lookup(99, eq_key("y")).map(|p| p.val.as_str()),
            Some("val_y")
        );
    }

    // HMap tests

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
        assert_eq!(map.lookup(h, eq_key("city")).map(|p| p.val.as_str()), Some("tokyo"));
    }

    #[test]
    fn hmap_remove_returns_value() {
        let mut map: HMap<KVPair> = HMap::with_capacity(4);
        let h = hash_key("lang");
        map.insert(h, kv("lang", "rust"));
        assert_eq!(map.remove(h, eq_key("lang")).map(|p| p.val), Some("rust".to_string()));
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
            assert!(map.lookup(hash_key(&key), eq_key(&key)).is_none(), "k{i} still present after remove");
        }
    }

    #[test]
    fn hmap_no_duplicates_after_migration() {
        // Each key should be removable exactly once after migration completes
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
