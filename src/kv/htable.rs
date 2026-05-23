use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::ptr;
use super::hnode::HNode;

pub struct HTab<T> {
    pub(super) tab: *mut *mut HNode<T>,
    pub(super) mask: usize,
    pub(super) size: usize,
}

impl<T: Clone> HTab<T> {
    pub fn with_capacity(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        let layout = Layout::array::<*mut HNode<T>>(capacity).unwrap();
        let slots = unsafe { alloc_zeroed(layout) as *mut *mut HNode<T> };
        HTab {
            mask: capacity - 1,
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
            if n.hcode == hash && eq(&n.val) {
                return Some(&n.val);
            }
            node = unsafe { n.next.as_ref() };
        }
        None
    }

    pub fn remove(&mut self, hash: usize, eq: impl Fn(&T) -> bool) -> Option<T> {
        let pos = hash & self.mask;
        unsafe {
            let mut prev: *mut *mut HNode<T> = self.tab.add(pos);
            let mut node = *prev;
            while !node.is_null() {
                if (*node).hcode == hash && eq(&(*node).val) {
                    *prev = (*node).next;
                    (*node).next = ptr::null_mut();
                    self.size -= 1;
                    return Some(Box::from_raw(node).val.clone());
                }
                prev = &raw mut (*node).next;
                node = (*node).next;
            }
        }
        None
    }
}

impl<T> Drop for HTab<T> {
    fn drop(&mut self) {
        let layout = Layout::array::<*mut HNode<T>>(self.mask + 1).unwrap();
        for i in 0..(self.mask + 1) {
            let head = unsafe { *self.tab.add(i) };
            if !head.is_null() {
                unsafe { drop(Box::from_raw(head)); }
            }
        }
        unsafe { dealloc(self.tab as *mut u8, layout); }
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
        tab.insert(0,  kv("a", "1"));
        tab.insert(8,  kv("b", "2"));
        tab.insert(16, kv("c", "3"));
        assert!(tab.lookup(0,  eq_key("a")).is_some());
        assert!(tab.lookup(8,  eq_key("b")).is_some());
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
        tab.insert(0,  kv("a", "1"));
        tab.insert(8,  kv("b", "2"));
        tab.insert(16, kv("c", "3"));
        tab.remove(8, eq_key("b"));
        assert!(tab.lookup(0,  eq_key("a")).is_some());
        assert!(tab.lookup(8,  eq_key("b")).is_none());
        assert!(tab.lookup(16, eq_key("c")).is_some());
    }

    #[test]
    fn multiple_keys_same_hcode() {
        let mut tab: HTab<KVPair> = HTab::with_capacity(8);
        tab.insert(99, kv("x", "val_x"));
        tab.insert(99, kv("y", "val_y"));
        assert_eq!(tab.lookup(99, eq_key("x")).map(|p| p.val.as_str()), Some("val_x"));
        assert_eq!(tab.lookup(99, eq_key("y")).map(|p| p.val.as_str()), Some("val_y"));
    }
}
