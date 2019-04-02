use super::atomicvec::AtomicVec;
use super::key::{KeyHolder, ValueHolder};
use std::hash::Hash;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

pub static REPROBE_LIMIT: usize = 10;

// ---Hash Table Layer Node -------------------------------------------------------------------------------
pub struct KVs<K, V> {
    pub _ks: AtomicVec<KeyHolder<K>>,
    pub _vs: AtomicVec<ValueHolder<V>>,
    pub _chm: CHM<K, V>,
    pub _hashes: Vec<u64>,
}

impl<K: Hash, V> KVs<K, V> {
    pub fn new(table_size: usize) -> KVs<K, V> {
        KVs {
            _ks: AtomicVec::with_capacity(table_size),
            _vs: AtomicVec::with_capacity(table_size),
            _chm: CHM::<K, V>::new(),
            _hashes: vec![0; table_size],
        }
    }

    pub fn get_key_nonatomic_at(&self, idx: usize) -> *mut KeyHolder<K> {
        self._ks.load(idx)
    }

    pub fn get_value_nonatomic_at(&self, idx: usize) -> *mut ValueHolder<V> {
        self._vs.load(idx)
    }

    pub fn table_full(&self, reprobe_cnt: usize) -> bool {
        reprobe_cnt >= REPROBE_LIMIT && self._chm._slots.load(Ordering::SeqCst) >= self._ks.len()
    }

    pub fn reprobe_limit(&self) -> usize {
        REPROBE_LIMIT + (self._ks.len() << 2)
    }

    pub fn len(&self) -> usize {
        self._ks.len()
    }
}

// ---Structure for resizing -------------------------------------------------------

pub struct CHM<K, V> {
    pub _newkvs: AtomicPtr<KVs<K, V>>,
    pub _size: AtomicUsize,
    pub _slots: AtomicUsize,
    pub _copy_done: AtomicUsize,
    pub _copy_idx: AtomicUsize,
    pub _resizer: AtomicUsize,
}

impl<K, V> CHM<K, V> {
    pub fn new() -> CHM<K, V> {
        CHM {
            _newkvs: AtomicPtr::new(ptr::null_mut()),
            _size: AtomicUsize::new(0),
            _slots: AtomicUsize::new(0),
            _copy_done: AtomicUsize::new(0),
            _copy_idx: AtomicUsize::new(0),
            _resizer: AtomicUsize::new(0),
        }
    }

    // FIXME: why "non atomic"?
    pub fn get_newkvs_nonatomic(&self) -> *mut KVs<K, V> {
        self._newkvs.load(Ordering::SeqCst)
    }
}

impl<K, V> Drop for CHM<K, V> {
    fn drop(&mut self) {
        let p = self._newkvs.load(Ordering::SeqCst);
        if !p.is_null() {
            drop(unsafe { Box::from_raw(p) });
        }
    }
}
