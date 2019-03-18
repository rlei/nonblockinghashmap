use std::cell::UnsafeCell;
use std::cmp::min;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ptr;
use std::string::ToString;
use std::sync::atomic::{AtomicPtr, Ordering};
use std::time::{Duration, Instant};

use keyvalue::{
    Key, KeyTypes::KeyEmpty, KeyTypes::KeyTombStone, KeyTypes::KeyType, Value,
    ValueTypes::ValueEmpty, ValueTypes::ValueTombStone, ValueTypes::ValueType,
};
use kvtable::{KVs, REPROBE_LIMIT};

mod keyvalue;
mod kvtable;

const MIN_SIZE_LOG: u32 = 3;
const MIN_SIZE: usize = 1 << MIN_SIZE_LOG;

const MEMORY_ORDERING: Ordering = Ordering::SeqCst;

#[derive(PartialEq)]
pub enum MatchingTypes {
    MatchAll,
    MatchAllNotEmpty,
    MatchValue,
    FromCopySlot,
}

#[derive(Debug)]
pub struct ConcurrentMap<K, V> {
    inner: UnsafeCell<NonBlockingHashMap<K, V>>,
}

unsafe impl<K, V> Sync for ConcurrentMap<K, V> {}

impl<K: Eq + Hash, V: Eq> ConcurrentMap<K, V> {
    pub fn new() -> ConcurrentMap<K, V> {
        ConcurrentMap {
            inner: UnsafeCell::new(NonBlockingHashMap::new()),
        }
    }

    pub fn new_with_size(initial_sz: usize) -> ConcurrentMap<K, V> {
        ConcurrentMap {
            inner: UnsafeCell::new(NonBlockingHashMap::new_with_size(initial_sz)),
        }
    }

    // "impl DerefMut for ConcurrentMap" won't work because of "deref(&mut self)"
    #[allow(clippy::mut_from_ref)]
    pub fn as_mut(&self) -> &mut NonBlockingHashMap<K, V> {
        unsafe { &mut *self.inner.get() }
    }
}

// ---Hash Map --------------------------------------------------------------------
#[derive(Debug)]
pub struct NonBlockingHashMap<K, V> {
    _kvs: AtomicPtr<KVs<K, V>>,
    //_reprobes: AtomicUint,
    _last_resize: Instant,
}

impl<K: Eq + Hash, V: Eq> NonBlockingHashMap<K, V> {
    pub fn new() -> NonBlockingHashMap<K, V> {
        NonBlockingHashMap::new_with_size(MIN_SIZE)
    }

    pub fn new_with_size(initial_sz: usize) -> NonBlockingHashMap<K, V> {
        let mut initial_sz = initial_sz;
        if initial_sz > 1024 * 1024 {
            initial_sz = 1024 * 1024;
        }
        let mut i = MIN_SIZE_LOG;
        while 1 << i < initial_sz << 2 {
            i += 1;
        }

        NonBlockingHashMap {
            _kvs: AtomicPtr::new(Box::into_raw(Box::new(KVs::<K, V>::new(1 << i)))),
            //_reprobes: AtomicUint::new(0),
            _last_resize: Instant::now(),
        }
    }

    pub fn get_table_nonatomic(&self) -> *mut KVs<K, V> {
        self._kvs.load(MEMORY_ORDERING)
    }

    unsafe fn resize(&self, kvs: *mut KVs<K, V>) -> *mut KVs<K, V> {
        //fence(MEMORY_ORDERING);
        if (*kvs)._chm.has_newkvs() {
            return (*kvs)._chm._newkvs.load(MEMORY_ORDERING);
        }

        let oldlen: usize = (*kvs).len();
        let sz = (*kvs)._chm._size.load(MEMORY_ORDERING);
        let mut newsz = sz;

        if sz >= oldlen >> 2 {
            newsz = oldlen << 1;
            if sz >= oldlen >> 1 {
                newsz = oldlen << 2;
            }
        }

        let tm = Instant::now();
        if newsz <= oldlen
            && tm.duration_since(self._last_resize) <= Duration::new(1, 0)
            && (*kvs)._chm._slots.load(MEMORY_ORDERING) >= sz << 1
        {
            newsz = oldlen << 1;
        }

        if newsz < oldlen {
            newsz = oldlen;
        }

        let mut log2 = MIN_SIZE_LOG;
        while 1 << log2 < newsz {
            log2 += 1
        }

        if (*kvs)._chm.has_newkvs() {
            return (*kvs)._chm._newkvs.load(MEMORY_ORDERING);
        }

        let mut newkvs = Box::into_raw(Box::new(KVs::<K, V>::new(1 << log2)));

        if (*kvs)._chm.has_newkvs() {
            return (*kvs)._chm._newkvs.load(MEMORY_ORDERING);
        }

        let oldkvs = (*kvs)._chm._newkvs.load(MEMORY_ORDERING);
        if (*kvs)
            ._chm
            ._newkvs
            .compare_and_swap(oldkvs, newkvs, MEMORY_ORDERING)
            == oldkvs
        {
            (*kvs)._chm._has_newkvs = true;
            self.rehash();
        } else {
            newkvs = (*kvs)._chm._newkvs.load(MEMORY_ORDERING);
        }
        newkvs
    }

    pub fn put<'a>(&mut self, key: K, newval: V) -> &'a V {
        unsafe { self.put_if_match(key, newval, MatchingTypes::MatchAll, None) }
    }

    unsafe fn put_if_match<'a>(
        &mut self,
        key: K,
        newval: V,
        matchingtype: MatchingTypes,
        expval: Option<V>,
    ) -> &'a V {
        let table = self.get_table_nonatomic();
        self.put_if_match_to_kvs(table, key, newval, matchingtype, expval)
    }

    unsafe fn put_if_match_to_kvs<'a>(
        &mut self,
        kvs: *mut KVs<K, V>,
        key: K,
        newval: V,
        matchingtype: MatchingTypes,
        expval: Option<V>,
    ) -> &'a V {
        let new_expval = expval.map(|v| Box::into_raw(Box::new(Value::<V>::new(v))));
        let returnval = self.put_if_match_impl(
            kvs,
            Box::into_raw(Box::new(Key::<K>::new(key))),
            Box::into_raw(Box::new(Value::<V>::new(newval))),
            matchingtype,
            new_expval,
        );
        &(*(*returnval)._value)
    }

    unsafe fn put_if_match_impl(
        &mut self,
        kvs: *mut KVs<K, V>,
        key: *mut Key<K>,
        putval: *mut Value<V>,
        matchingtype: MatchingTypes,
        expval: Option<*mut Value<V>>,
    ) -> *mut Value<V> {
        //let mut debugval = 0 as *mut Value<V>;
        //if expval.is_some() { debugval = expval.unwrap() }
        assert!(!(*putval).is_empty()); // Never put a ValueEmpty type
        assert!(!(*putval).is_prime()); // Never put a Prime type
        assert!(matchingtype != MatchingTypes::MatchValue || !expval.is_none()); // If matchingtype==MatchValue then expval must contain something
        if !expval.is_none() {
            assert!(!(*expval.unwrap()).is_prime());
        } // Never expect a Prime type

        let mut hasher = DefaultHasher::new();
        (*key).hash(&mut hasher);
        let fullhash = hasher.finish();
        let len = (*kvs).len();
        let mut idx: usize = fullhash as usize & (len - 1);
        let mut reprobe_cnt: usize = 0;
        let mut k = (*kvs).get_key_nonatomic_at(idx);
        let mut v = (*kvs).get_value_nonatomic_at(idx);
        // Determine if expval is empty
        let mut expval_not_empty = false;
        if matchingtype == MatchingTypes::MatchValue {
            if !(*expval.unwrap()).is_empty() {
                expval_not_empty = true;
            }
        } else {
            expval_not_empty = true;
        }
        // Probing/Re-probing
        loop {
            if (*k).is_empty() {
                // Found an available key slot
                if (*putval).is_tombstone() {
                    return putval;
                } // Never change KeyEmpty to KeyTombStone
                if (*kvs)._ks[idx].compare_and_swap(k, key, MEMORY_ORDERING) == k {
                    // Add key to the slot
                    (*kvs)._chm._slots.fetch_add(1, MEMORY_ORDERING); // Add 1 to the number of used slots
                    (*kvs)._hashes[idx] = fullhash;
                    break;
                }
                k = (*kvs).get_key_nonatomic_at(idx);
                v = (*kvs).get_value_nonatomic_at(idx);
                assert!(!(*k).is_empty());
            }
            //fence(MEMORY_ORDERING);
            if k == key || (*k) == (*key) {
                break;
            }
            // Start re-probing
            reprobe_cnt += 1;
            if reprobe_cnt >= REPROBE_LIMIT || (*key).is_tombstone() {
                // Enter state {KeyTombStone, Empty}; steal exucution path for optimization; let helper save the day.
                let newkvs = self.resize(kvs);
                if expval_not_empty {
                    self.help_copy();
                }
                return self.put_if_match_impl(newkvs, key, putval, matchingtype, expval); // Put in the new table instead
            }
            idx = (idx + 1) & (len - 1);
            k = (*kvs).get_key_nonatomic_at(idx);
            v = (*kvs).get_value_nonatomic_at(idx);
        }
        // End probe/re-probing

        if (*putval) == (*v) {
            return v;
        } // Steal path exucution for optimization; let helper save the day.
        if (*kvs)._chm.has_newkvs()
            && (( (*v).is_tombstone() && (*kvs).table_full(reprobe_cnt) ) || // Resize if the table is full.
				(*v).is_prime())
        // I don't understand this, but I take it from the original code anyway. It is some sort of invalid state caused by compilier's optimization.
        {
            self.resize(kvs);
        }
        if (*kvs)._chm.has_newkvs() {
            // Check for the last time if kvs is the newest table
            let expval_is_empty = {
                match expval {
                    Some(val) => {
                        if (*val).is_empty() {
                            true
                        } else {
                            false
                        }
                    }
                    None => true,
                }
            };
            let copied_kvs = self.copy_slot_and_check(kvs, idx, !expval_is_empty); // If expval is empty then don't help (expval is empty only if this function is called from copy_slot)
            return self.put_if_match_impl(copied_kvs, key, putval, matchingtype, expval);
        }

        // This table is the newest, so we can start entering the state machine.
        loop {
            assert!(!(*v).is_prime()); // If there is a Prime than this cannot be the newest table.
            if matchingtype!=MatchingTypes::MatchAll && // If expval is not a wildcard
				!( matchingtype==MatchingTypes::MatchAllNotEmpty && !(*v).is_tombstone() && !(*v).is_empty() )
            // If expval is not a TombStone or Empty
            {
                assert!(!expval.is_none());
                assert!(matchingtype == MatchingTypes::MatchValue);
                if v!=expval.unwrap() && // if v!= expval (pointer)
						!((*v).is_empty() && (*expval.unwrap()).is_tombstone()) && // If we expect a TombStone and v is empty, it should be a match.
								*expval.unwrap()!=*v
                // expval==Empty or *expval==*v
                {
                    return v; // do nothing, just return the old value.
                }
            }

            // Finally, add some values.
            if (*kvs)._vs[idx].compare_and_swap(v, putval, MEMORY_ORDERING) == v {
                if expval_not_empty {
                    if ((*v).is_empty() || (*v).is_tombstone()) && !(*putval).is_tombstone() {
                        (*kvs)._chm._size.fetch_add(1, MEMORY_ORDERING);
                    }
                    if !((*v).is_empty() || (*v).is_tombstone()) && (*putval).is_tombstone() {
                        (*kvs)._chm._size.fetch_sub(1, MEMORY_ORDERING);
                    }
                }
                if (*v).is_empty() && expval_not_empty {
                    return Box::into_raw(Box::new(Value::<V>::new_tombstone()));
                } else {
                    return v;
                }
            }
            v = (*kvs).get_value_nonatomic_at(idx);
            if (*v).is_prime() {
                let copied_kvs = self.copy_slot_and_check(kvs, idx, expval_not_empty);
                return self.put_if_match_impl(copied_kvs, key, putval, matchingtype, expval);
            }
        }
    }

    pub fn get<'a>(&'a mut self, key: K) -> Option<&'a V> {
        let table = self.get_table_nonatomic();
        let maybe_val =
            unsafe { self.get_impl(table, Box::into_raw(Box::new(Key::<K>::new(key)))) };
        maybe_val.map(|v| unsafe { &*(*v)._value })
    }

    // Compute hash only once
    unsafe fn get_impl(&mut self, kvs: *mut KVs<K, V>, key: *mut Key<K>) -> Option<*mut Value<V>> {
        let mut hasher = DefaultHasher::new();
        (*key).hash(&mut hasher);
        let fullhash = hasher.finish();
        self.get_impl_supply_hash(kvs, key, fullhash)
    }

    unsafe fn get_impl_supply_hash(
        &mut self,
        kvs: *mut KVs<K, V>,
        key: *mut Key<K>,
        fullhash: u64,
    ) -> Option<*mut Value<V>> {
        let len = (*kvs).len();
        let mut idx = (fullhash & (len - 1) as u64) as usize;
        let mut reprobe_cnt: usize = 0;
        loop {
            let k = (*kvs).get_key_nonatomic_at(idx);
            let v = (*kvs).get_value_nonatomic_at(idx);
            if (*k).is_empty() {
                return None;
            }
            //fence(MEMORY_ORDERING);
            if (*k) == (*key) {
                if !(*v).is_prime() {
                    if (*v).is_tombstone() {
                        return None;
                    } else {
                        return Some(v);
                    }
                } else {
                    let table = self.copy_slot_and_check(kvs, idx, true);
                    return self.get_impl_supply_hash(table, key, fullhash);
                }
            }
            reprobe_cnt += 1;
            if reprobe_cnt >= REPROBE_LIMIT || (*k).is_tombstone() {
                if (*kvs)._chm.has_newkvs() {
                    self.help_copy();
                    return self.get_impl_supply_hash(
                        (*kvs)._chm.get_newkvs_nonatomic(),
                        key,
                        fullhash,
                    );
                } else {
                    return None;
                }
            }
            idx = (idx + 1) & (len - 1);
        }
    }

    unsafe fn copy_slot_and_check(
        &mut self,
        oldkvs: *mut KVs<K, V>,
        idx: usize,
        should_help: bool,
    ) -> *mut KVs<K, V> {
        //fence(MEMORY_ORDERING);
        assert!((*oldkvs)._chm.get_newkvs_nonatomic() != ptr::null_mut());
        if self.copy_slot(oldkvs, idx) {
            self.copy_check_and_promote(oldkvs, 1);
        }

        if should_help {
            self.help_copy();
        }
        (*oldkvs)._chm.get_newkvs_nonatomic()
    }

    unsafe fn copy_check_and_promote(&mut self, oldkvs: *mut KVs<K, V>, work_done: usize) {
        let oldlen = (*oldkvs).len();
        let mut copy_done = (*oldkvs)._chm._copy_done.load(MEMORY_ORDERING);
        assert!(copy_done + work_done <= oldlen);
        if work_done > 0 {
            while (*oldkvs)._chm._copy_done.compare_and_swap(
                copy_done,
                copy_done + work_done,
                MEMORY_ORDERING,
            ) != copy_done
            {
                copy_done = (*oldkvs)._chm._copy_done.load(MEMORY_ORDERING);
            }
            assert!(copy_done + work_done <= oldlen);
        }

        if copy_done + work_done == oldlen
            && self._kvs.load(MEMORY_ORDERING) == oldkvs
            && (self._kvs.compare_and_swap(
                oldkvs,
                (*oldkvs)._chm.get_newkvs_nonatomic(),
                MEMORY_ORDERING,
            ) == oldkvs)
        {
            //println!("---obsolete---")
            //print_kvs(oldkvs);
            self._last_resize = Instant::now();
        }
    }

    unsafe fn copy_slot(&mut self, oldkvs: *mut KVs<K, V>, idx: usize) -> bool {
        let mut key = (*oldkvs).get_key_nonatomic_at(idx);

        // State transition: {Empty, Empty} -> {KeyTombStone, Empty}
        // ---------------------------------------------------------
        let tombstone_ptr: *mut Key<K> = Box::into_raw(Box::new(Key::<K>::new_tombstone()));
        while (*key).is_empty() {
            if (*oldkvs)._ks[idx].compare_and_swap(key, tombstone_ptr, MEMORY_ORDERING) == key {
                // Attempt {Empty, Empty} -> {KeyTombStone, Empty}
                return true;
            }
            key = (*oldkvs).get_key_nonatomic_at(idx);
        }
        // ---------------------------------------------------------

        // Enter state: {KeyTombStone, Empty}
        // ---------------------------------------------------------
        if (*key).is_tombstone() {
            return false;
        }
        // ---------------------------------------------------------

        // State transition: {Key, Empty} -> {Key, ValueTombPrime} or {Key, ValueTombStone} -> {Key, ValueTombPrime} or {Key, Value}->{Key, Value.get_prime()}
        // -------------------------------------------------------------------------------------------------------
        let tombstone_ptr = Value::<V>::new_tombstone().get_prime();
        let mut oldvalue = (*oldkvs).get_value_nonatomic_at(idx);
        while !(*oldvalue).is_prime() {
            let primed = {
                if (*oldvalue).is_empty() {
                    tombstone_ptr
                } else {
                    (*oldvalue).get_prime()
                }
            };
            if (*oldkvs)._vs[idx].compare_and_swap(oldvalue, primed, MEMORY_ORDERING) == oldvalue {
                if (*primed).valuetype() == ValueTombStone {
                    return true;
                }
                // Transition: {Key, Empty} -> {Key, ValueTombPrime} or {Key, ValueTombStone} -> {Key, ValueTombPrime}
                else {
                    // Transition: {Key, Value} -> {Key, Value'}
                    oldvalue = primed;
                    break;
                }
            }
            oldvalue = (*oldkvs).get_value_nonatomic_at(idx);
        }
        // -------------------------------------------------------------------------------------------------------

        let tombprime = Value::<V>::new_tombprime();

        // Enter state: {Key, ValueTombPrime}
        // ---------------------------------------------------------
        if (*oldvalue).is_tombprime() {
            return false;
        }
        // ---------------------------------------------------------

        // State transition: {Key, Value.get_prime()} -> {KeyTombStone, ValueTombPrime}
        // ---------------------------------------------------------
        let old_unprimed = (*oldvalue).get_unprime();
        assert!((*old_unprimed) != tombprime);
        let newkvs = (*oldkvs)._chm.get_newkvs_nonatomic();
        let emptyval: *mut Value<V> = Box::into_raw(Box::new(Value::<V>::new_empty()));

        self.put_if_match_impl(
            newkvs,
            key,
            old_unprimed,
            MatchingTypes::MatchValue,
            Some(emptyval),
        );

        let tombprime_ptr: *mut Value<V> = Box::into_raw(Box::new(Value::<V>::new_tombprime()));

        // Enter state: {Key, Value.get_prime()} (intermediate)
        oldvalue = (*oldkvs).get_value_nonatomic_at(idx); // Check again, just in case...
        while !(*oldvalue).is_tombprime() {
            if (*oldkvs)._vs[idx].compare_and_swap(oldvalue, tombprime_ptr, MEMORY_ORDERING)
                == oldvalue
            {
                return true;
            }
            oldvalue = (*oldkvs).get_value_nonatomic_at(idx);
        }
        // ---------------------------------------------------------

        false // State jump to {KeyTombStone, ValueTombPrime} for threads that lost the competition
    }

    unsafe fn help_copy(&mut self) {
        if (*self.get_table_nonatomic())._chm.has_newkvs() {
            let kvs: *mut KVs<K, V> = self.get_table_nonatomic();
            self.help_copy_impl(kvs, false);
        }
    }

    unsafe fn help_copy_impl(&mut self, oldkvs: *mut KVs<K, V>, copy_all: bool) {
        //fence(MEMORY_ORDERING);
        assert!((*oldkvs)._chm.has_newkvs());
        let oldlen = (*oldkvs).len();
        let min_copy_work = min(oldlen, 1024);
        let mut panic_start = false;
        let mut copy_idx: usize = 0;

        while (*oldkvs)._chm._copy_done.load(MEMORY_ORDERING) < oldlen {
            if !panic_start {
                copy_idx = (*oldkvs)._chm._copy_idx.load(MEMORY_ORDERING);
                while copy_idx < oldlen << 1
                    && (*oldkvs)._chm._copy_idx.compare_and_swap(
                        copy_idx,
                        copy_idx + min_copy_work,
                        MEMORY_ORDERING,
                    ) != copy_idx
                {
                    copy_idx = (*oldkvs)._chm._copy_idx.load(MEMORY_ORDERING);
                }
                if copy_idx >= oldlen << 1 {
                    panic_start = true;
                }
            }
            //for i in range (0, min_copy_work){
            //if (*oldkvs)._chm.has_newkvs() {
            //self.copy_slot_and_check(oldkvs, (copy_idx+i)&(oldlen-1), false) ;
            //}
            //}
            let mut work_done = 0;
            for i in 0..min_copy_work {
                if self.copy_slot(oldkvs, (copy_idx + i) & (oldlen - 1)) {
                    work_done += 1;
                }
            }
            if work_done > 0 {
                self.copy_check_and_promote(oldkvs, work_done);
            }

            copy_idx += min_copy_work;

            if !copy_all && !panic_start {
                return;
            }
        }
        self.copy_check_and_promote(oldkvs, 0);
    }

    pub fn get_kvs_level(&self, level: u32) -> Option<*mut KVs<K, V>> {
        NonBlockingHashMap::get_kvs_level_impl(self.get_table_nonatomic(), level)
    }

    fn get_kvs_level_impl(kvs: *mut KVs<K, V>, level: u32) -> Option<*mut KVs<K, V>> {
        if kvs.is_null() {
            return None;
        }
        if level == 0 {
            Some(kvs)
        } else {
            unsafe {
                NonBlockingHashMap::get_kvs_level_impl(
                    (*kvs)._chm.get_newkvs_nonatomic(),
                    level - 1,
                )
            }
        }
    }

    unsafe fn fast_keyeq(k: *mut Key<K>, hashk: u64, key: *mut Key<K>, hashkey: u64) -> bool {
        k == key || ((hashk == 0 || hashk == hashkey) && !(*k).is_tombstone() && (*key) == (*k))
    }

    pub fn rehash(&self) {}

    pub fn capacity(&self) -> usize {
        unsafe { (*self._kvs.load(MEMORY_ORDERING)).len() }
    }
}

// debuging functions
unsafe fn print_table<K: Eq + Hash + ToString, V: Eq + ToString>(table: &NonBlockingHashMap<K, V>) {
    print_kvs(table.get_table_nonatomic());
}

pub fn print_all<K: Eq + Hash + ToString, V: Eq + ToString>(table: &NonBlockingHashMap<K, V>) {
    let mut kvs = table.get_table_nonatomic();
    let mut i = 0;
    while !kvs.is_null() {
        println!("---Table {}---", i);
        unsafe { print_kvs(kvs) };
        i += 1;
        kvs = unsafe { (*kvs)._chm.get_newkvs_nonatomic() };
    }
}

unsafe fn print_kvs<K: Eq + Hash + ToString, V: Eq + ToString>(kvs: *mut KVs<K, V>) {
    for i in 0..(*kvs).len() {
        print!(
            "{}: ({}, ",
            i,
            key_to_string((*kvs).get_key_nonatomic_at(i))
        );
        print!("{}, ", value_to_string((*kvs).get_value_nonatomic_at(i)));
        println!("{})", (*kvs)._hashes[i]);
    }
}

unsafe fn key_to_string<K: Eq + Hash + ToString>(key: *mut Key<K>) -> String {
    match (*key).keytype() {
        KeyTombStone => String::from("TOMBSTONE"),
        KeyEmpty => String::from("EMPTY"),
        KeyType => {
            assert!(!(*key)._key.is_null());
            (*(*key)._key).to_string()
        }
    }
}

unsafe fn value_to_string<V: Eq + ToString>(value: *mut Value<V>) -> String {
    match (*value).valuetype() {
        ValueTombStone => {
            if (*value).is_prime() {
                String::from("TOMBPRIME")
            } else {
                String::from("TOMBSTONE")
            }
        }
        ValueEmpty => String::from("EMPTY"),
        ValueType => {
            assert!(!(*value)._value.is_null());
            let val_str = (*(*value)._value).to_string();
            if (*value).is_prime() {
                format!("Prime({})", val_str)
            } else {
                val_str
            }
        }
    }
}

/****************************************************************************
 * Tests
 ****************************************************************************/
#[cfg(test)]
mod test {
    use super::{
        ConcurrentMap, KVs, Key, KeyEmpty, NonBlockingHashMap, Value, ValueEmpty, MEMORY_ORDERING,
    };
    use std::sync::atomic::AtomicPtr;
    use std::sync::Arc;
    use std::thread::{sleep, spawn};
    use std::time::Duration;

    #[test]
    fn test_value_prime_swapping() {
        unsafe {
            let value: *mut Value<i32> = Box::into_raw(Box::new(Value::new(10)));
            let atomicvalue = AtomicPtr::new(value);
            let valueprime = (*value).get_prime();
            assert!(!(*atomicvalue.load(MEMORY_ORDERING)).is_prime());
            atomicvalue.swap(valueprime, MEMORY_ORDERING);
            assert!((*atomicvalue.load(MEMORY_ORDERING))._value == (*value)._value);
            assert!((*atomicvalue.load(MEMORY_ORDERING)).is_prime());
        }
    }

    #[test]
    fn test_kv_destroy() {
        unsafe {
            let mut p: *mut i32 = Box::into_raw(Box::new(5));
            {
                let kv = Key::new(10);
                p = kv.get_key();
                assert!((*p) == 10);
            }
            assert!((*p) != 10);
            assert!((*p) != 5);

            let mut p: *mut i32 = Box::into_raw(Box::new(5));
            {
                let kv = Value::new(10);
                p = kv.get_value();
                assert!((*p) == 10);
            }
            assert!((*p) != 10);
            assert!((*p) != 5);
        }
    }

    #[test]
    fn test_key_eq() {
        assert!(Key::<i32>::new_empty() == Key::<i32>::new_empty());
        assert!(Key::<i32>::new_tombstone() == Key::<i32>::new_tombstone());
        assert!(Key::<i32>::new(10) == Key::<i32>::new(10));
        assert!(Key::<i32>::new(5) != Key::<i32>::new(10));
    }

    #[test]
    fn test_value_eq() {
        unsafe {
            assert!(Value::<i32>::new_empty() == Value::<i32>::new_empty());
            assert!(Value::<i32>::new_tombstone() == Value::<i32>::new_tombstone());
            assert!(
                (*Value::<i32>::new_tombstone().get_prime())
                    == (*Value::<i32>::new_tombstone().get_prime())
            );
            assert!(Value::<i32>::new_tombprime() == (*Value::<i32>::new_tombstone().get_prime()));
            assert!(Value::<i32>::new_tombprime() == Value::<i32>::new_tombprime());
            assert!(Value::<i32>::new(10) == Value::<i32>::new(10));
            assert!(Value::<i32>::new(5) != Value::<i32>::new(10));
            assert!((*Value::<i32>::new(10).get_prime()) == (*Value::<i32>::new(10).get_prime()));
        }
    }

    #[test]
    fn test_kvs_init() {
        let kvs = KVs::<i32, i32>::new(10);
        unsafe {
            for i in 0..kvs._ks.len() {
                assert_eq!((*kvs._ks[i].load(MEMORY_ORDERING)).keytype(), KeyEmpty);
            }
            for i in 0..kvs._ks.len() {
                assert_eq!((*kvs._vs[i].load(MEMORY_ORDERING)).valuetype(), ValueEmpty);
            }
        }
    }

    #[test]
    fn test_hashmap_init() {
        let map = NonBlockingHashMap::<i32, i32>::new_with_size(10);
        assert!(map.capacity() == 16 * 4);
        unsafe {
            assert!((*map._kvs.load(MEMORY_ORDERING))
                ._chm
                ._newkvs
                .load(MEMORY_ORDERING)
                .is_null());
        }
    }

    #[test]
    fn test_hashmap_resize() {
        let map1 = NonBlockingHashMap::<i32, i32>::new_with_size(10);
        let kvs = map1._kvs.load(MEMORY_ORDERING);
        unsafe {
            map1.resize(kvs);
            assert_eq!(
                (*(*kvs)._chm._newkvs.load(MEMORY_ORDERING)).len(),
                16 * 4 * 2
            );
            let kvs = (*kvs)._chm._newkvs.load(MEMORY_ORDERING);
            map1.resize(kvs);
            assert_eq!(
                (*(*kvs)._chm._newkvs.load(MEMORY_ORDERING)).len(),
                16 * 4 * 4
            );
        }
        let map2 = NonBlockingHashMap::<i32, i32>::new_with_size(10);
        sleep(Duration::from_millis(2000));
        unsafe {
            map2.resize(map2._kvs.load(MEMORY_ORDERING));
            let new_len = (*(*map2._kvs.load(MEMORY_ORDERING))
                ._chm
                ._newkvs
                .load(MEMORY_ORDERING))
            .len();
            assert_eq!(new_len, 16 * 4);
        }
    }

    #[test]
    fn test_hashmap_single_thread_grow() {
        let map = ConcurrentMap::new_with_size(10);
        for n in 0..200_000 {
            map.as_mut().put(n, n);
        }
        for n in 0..200_000 {
            assert_eq!(n, *map.as_mut().get(n).unwrap());
        }
    }

    fn test_hashmap_concurrent(init_size: usize, nthreads: usize, num_keys: usize) {
        let shared_map = Arc::new(ConcurrentMap::new_with_size(init_size));

        let threads: Vec<_> = (0..nthreads)
            .flat_map(|_| {
                let child_map_put = shared_map.clone();
                let child_map_get = shared_map.clone();
                let writer = spawn(move || {
                    for i in 0..num_keys {
                        child_map_put
                            .as_mut()
                            .put(format!("key {}", i), format!("value {}", i));
                    }
                });

                let reader = spawn(move || {
                    sleep(Duration::from_millis(10));
                    let mut hit = 0;
                    for i in 0..num_keys {
                        let key = format!("key {}", i);
                        if let Some(v) = child_map_get.as_mut().get(key) {
                            assert_eq!(*v, format!("value {}", i));
                            hit += 1;
                        }
                    }
                    assert!(hit > 0);
                });
                vec![writer, reader]
            })
            .collect();
        for t in threads {
            t.join().expect("Error joining");
        }
    }

    #[test]
    fn test_hashmap_concurrent_rw_no_resize() {
        test_hashmap_concurrent(100_000, 8, 100_000);
    }

    #[test]
    fn test_hashmap_concurrent_rw_grow() {
        test_hashmap_concurrent(16, 8, 100_000);
    }
}
