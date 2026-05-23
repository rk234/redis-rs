use std::ptr;

pub(super) struct HNode<T> {
    pub(super) next: *mut HNode<T>,
    pub(super) hcode: usize,
    pub(super) val: T,
}

impl<T> HNode<T> {
    pub(super) fn new(val: T, hcode: usize) -> HNode<T> {
        HNode {
            next: ptr::null_mut(),
            hcode,
            val,
        }
    }
}

impl<T> Drop for HNode<T> {
    fn drop(&mut self) {
        if !self.next.is_null() {
            unsafe { drop(Box::from_raw(self.next)) }
        }
    }
}
