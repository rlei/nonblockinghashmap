use std::hash::{Hash, Hasher};
use std::ptr;

// ---Key-or-Value Slot Type--------------------------------------------------------------------------------
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum KeyTypes {
    KeyType,
    KeyTombStone,
    KeyEmpty,
}

pub struct Key<T> {
    // TODO: instead of having a key type field, should make Key a sum type
    pub _keytype: KeyTypes,
    pub _key: *mut T,
}

impl<T: Hash> Key<T> {
    pub fn new(k: T) -> Key<T> {
        Key {
            _keytype: KeyTypes::KeyType,
            _key: Box::into_raw(Box::new(k)),
        }
    }

    //pub fn new_pointer(k: *T) -> Key<T> {
    //Key { _keytype: KeyType, _key: k }
    //}

    pub fn new_empty() -> Key<T> {
        Key {
            _keytype: KeyTypes::KeyEmpty,
            _key: ptr::null_mut(),
        }
    }

    pub fn new_tombstone() -> Key<T> {
        Key {
            _keytype: KeyTypes::KeyTombStone,
            _key: ptr::null_mut(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self._keytype == KeyTypes::KeyEmpty
    }

    pub fn is_tombstone(&self) -> bool {
        self._keytype == KeyTypes::KeyTombStone
    }

    pub fn keytype(&self) -> KeyTypes {
        self._keytype
    }

    pub fn get_key(&self) -> *mut T {
        assert!(!self._key.is_null());
        self._key
    }

    // ---Hash Function--------------------------------------------------------------------------------------
    /*
    pub fn hash(&self) -> u64 {
        let mut h = hash::hash(&(*self._key));
        h += (h << 15) ^ 0xffffcd7d;
        h ^= h >> 10;
        h += h << 3;
        h ^= h >> 6;
        h += h << 2 + h << 14;
        return h ^ (h >> 16);
    }
    */
}

impl<T: Hash> Hash for Key<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { (*self._key).hash(state) };
    }
}

impl<T> Drop for Key<T> {
    fn drop(&mut self) {
        if self._keytype == KeyTypes::KeyType {
            drop(unsafe { Box::from_raw(self._key) });
        }
    }
}

impl<T: PartialEq + Hash> PartialEq for Key<T> {
    fn eq(&self, other: &Key<T>) -> bool {
        if self._keytype != other._keytype {
            return false;
        }
        if self._keytype == KeyTypes::KeyEmpty && other._keytype == KeyTypes::KeyEmpty {
            return true;
        }
        if self._keytype == KeyTypes::KeyTombStone && other._keytype == KeyTypes::KeyTombStone {
            return true;
        }
        assert!(!self._key.is_null() && !other._key.is_null());
        if self._key == other._key || unsafe { *self._key == *other._key } {
            return true;
        }
        return false;
    }
}

#[derive(PartialEq, Copy, Clone, Debug)]
pub enum ValueTypes {
    ValueType,
    ValueTombStone,
    ValueEmpty,
}

pub struct Value<T> {
    // TODO: instead of having a key type field, should make Value a sum type
    pub _valuetype: ValueTypes,
    pub _value: *mut T,
    pub _is_prime: bool,
}

impl<T> Value<T> {
    pub fn new(v: T) -> Value<T> {
        Value {
            _valuetype: ValueTypes::ValueType,
            _value: Box::into_raw(Box::new(v)),
            _is_prime: false,
        }
    }

    pub fn new_empty() -> Value<T> {
        Value {
            _valuetype: ValueTypes::ValueEmpty,
            _value: ptr::null_mut(),
            _is_prime: false,
        }
    }

    pub fn new_tombstone() -> Value<T> {
        Value {
            _valuetype: ValueTypes::ValueTombStone,
            _value: ptr::null_mut(),
            _is_prime: false,
        }
    }

    pub fn new_tombprime() -> Value<T> {
        Value {
            _valuetype: ValueTypes::ValueTombStone,
            _value: ptr::null_mut(),
            _is_prime: true,
        }
    }

    pub fn new_prime(v: T) -> Value<T> {
        Value {
            _valuetype: ValueTypes::ValueType,
            _value: Box::into_raw(Box::new(v)),
            _is_prime: true,
        }
    }

    pub fn is_empty(&self) -> bool {
        assert!(self._value.is_null() == (self._valuetype == ValueTypes::ValueEmpty));
        self._valuetype == ValueTypes::ValueEmpty
    }

    pub fn is_tombstone(&self) -> bool {
        self._valuetype == ValueTypes::ValueTombStone
    }

    pub fn is_prime(&self) -> bool {
        self._is_prime
    }

    pub fn is_tombprime(&self) -> bool {
        self.is_prime() && self.is_tombstone()
    }

    pub fn get_prime(&self) -> *mut Value<T> {
        assert!(!self.is_prime());
        Box::into_raw(Box::new(Value {
            _valuetype: self._valuetype,
            _value: self._value,
            _is_prime: true,
        }))
    }

    pub fn get_unprime(&self) -> *mut Value<T> {
        assert!(self.is_prime());
        Box::into_raw(Box::new(Value {
            _valuetype: self._valuetype,
            _value: self._value,
            _is_prime: false,
        }))
    }

    pub fn valuetype(&self) -> ValueTypes {
        self._valuetype
    }

    pub fn get_value(&self) -> *mut T {
        self._value
    }
}

impl<T> Drop for Value<T> {
    fn drop(&mut self) {
        if self._valuetype == ValueTypes::ValueType {
            drop(unsafe { Box::from_raw(self._value) });
        }
    }
}

impl<T: PartialEq> PartialEq for Value<T> {
    fn eq(&self, other: &Value<T>) -> bool {
        if self._valuetype != other._valuetype {
            return false;
        }
        if self._valuetype == ValueTypes::ValueEmpty && other._valuetype == ValueTypes::ValueEmpty {
            return true;
        }
        if self._valuetype == ValueTypes::ValueTombStone
            && other._valuetype == ValueTypes::ValueTombStone
            && self._is_prime == other._is_prime
        {
            return true;
        }
        assert!(!self._value.is_null() && !other._value.is_null());
        if (self._value == other._value || unsafe { *self._value == *other._value })
            && self._is_prime == other._is_prime
        {
            return true;
        }
        return false;
    }
}

#[cfg(test)]
mod tests {
    use super::{Key, Value};

    #[test]
    fn test_key_drop() {
        drop(Key::new(42));
        drop(Key::new(String::from("Hello")));
    }

    #[test]
    fn test_key_empty_tombstone_drop() {
        drop(Key::<String>::new_empty());
        drop(Key::<String>::new_tombstone());
    }

    #[test]
    fn test_value_drop() {
        drop(Value::new(42));
        drop(Value::new_prime(42));
        drop(Value::new(String::from("Hello")));
        drop(Value::new_prime(String::from("Hello")));
        drop(Value::new(String::from("Hello")).get_prime());
        drop(Value::new_prime(String::from("Hello")).get_unprime());
    }

    #[test]
    fn test_value_tombstones_drop() {
        let value = Value::<String>::new_tombstone();
        drop(value);
        let value = Value::<String>::new_tombstone().get_prime();
        drop(value);
        let value = Value::<String>::new_tombprime();
        drop(value);
        let value = Value::<String>::new_tombprime().get_unprime();
        drop(value);
    }
}
