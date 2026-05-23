use std::hash::Hash;

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
