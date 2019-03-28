use std::intrinsics;

pub struct AtomicVec<T> {
    v: Vec<*mut T>,
}

impl<T> AtomicVec<T> {
    pub fn with_capacity(size: usize) -> AtomicVec<T> {
        AtomicVec { v: vec![std::ptr::null_mut(); size] }
    }

    pub fn load(&self, index: usize) -> *mut T {
        assert!(index < self.v.len());
        unsafe { intrinsics::atomic_load(self.v.as_ptr().offset(index as isize) as *const usize) as *mut T }
    }

    pub fn cas(&mut self, index: usize, old: *mut T, val: *mut T) -> *mut T {
        assert!(index < self.v.len());
        let (val, ok) = unsafe { intrinsics::atomic_cxchg(self.v.as_mut_ptr().offset(index as isize) as *mut usize,
            old as usize, val as usize) };
        val as *mut T
    }
}

mod tests {
    use super::AtomicVec;

    #[test]
    fn test_load() {
        let v = AtomicVec::<i32>::with_capacity(100);
        assert!(v.load(10).is_null());
        assert!(v.load(99).is_null());
    }

    #[test]
    fn test_cas() {
        let mut v = AtomicVec::with_capacity(100);
        let p = Box::into_raw(Box::new(5));
        assert!(v.cas(10, std::ptr::null_mut(), p).is_null());

        let p1 = Box::into_raw(Box::new(42));
        assert_eq!(p, v.cas(10, p, p1));
        unsafe { Box::from_raw(p) };

        assert_eq!(p1, v.cas(10, p1, std::ptr::null_mut()));
        assert!(v.cas(10, p1, std::ptr::null_mut()).is_null());
        unsafe { Box::from_raw(p1) };
    }
}