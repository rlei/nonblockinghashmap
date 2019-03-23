use super::keyvalue::{Key, Value};
use std::hash::Hash;
use std::ptr;
use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

pub static REPROBE_LIMIT: usize = 10;

// ---Hash Table Layer Node -------------------------------------------------------------------------------
pub struct KVs<K, V> {
    pub _ks: Vec<AtomicPtr<Key<K>>>,
    pub _vs: Vec<AtomicPtr<Value<V>>>,
    pub _chm: CHM<K, V>,
    pub _hashes: Vec<u64>,
}

impl<K: Hash, V> KVs<K, V> {
    pub fn new(table_size: usize) -> KVs<K, V> {
        KVs {
            _ks: {
                let mut temp = Vec::with_capacity(table_size);
                for _ in 0..table_size {
                    temp.push(AtomicPtr::new(Box::into_raw(Box::new(
                        Key::<K>::new_empty(),
                    ))));
                }
                temp
            },
            _vs: {
                let mut temp = Vec::with_capacity(table_size);
                for _ in 0..table_size {
                    temp.push(AtomicPtr::new(Box::into_raw(Box::new(
                        Value::<V>::new_empty(),
                    ))));
                }
                temp
            },
            _chm: CHM::<K, V>::new(),
            _hashes: vec![0; table_size],
        }
    }

    pub fn get_key_nonatomic_at(&self, idx: usize) -> *mut Key<K> {
        self._ks[idx].load(Ordering::SeqCst)
    }

    pub fn get_value_nonatomic_at(&self, idx: usize) -> *mut Value<V> {
        self._vs[idx].load(Ordering::SeqCst)
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

impl<K, V> Drop for KVs<K, V> {
    fn drop(&mut self) {
        for i in 0..self._ks.len() {
            drop(unsafe { Box::from_raw(self._ks[i].load(Ordering::SeqCst)) });
            drop(unsafe { Box::from_raw(self._vs[i].load(Ordering::SeqCst)) });
        }
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
