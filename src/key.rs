//use std::hash::Hash;

#[derive(PartialEq, Hash, Debug)]
pub enum KeyHolder<T> {
    Key(T),
    Tombstone,
}

impl<T: PartialEq> KeyHolder<T> {
    pub fn is_tombstone(&self) -> bool {
        *self == KeyHolder::Tombstone
    }
}

#[derive(PartialEq, Hash, Debug)]
pub enum ValueHolder<T> {
    Value(T),
    Tombstone,
    Prime(Box<ValueHolder<T>>),     // not for direct instantiation
}

impl<T> ValueHolder<T> {
    pub fn is_tombstone(&self) -> bool {
        match self {
            ValueHolder::Tombstone => true,
            ValueHolder::Prime(box ValueHolder::Tombstone) => true,
            _ => false,
        }
    }

    pub fn is_prime(&self) -> bool {
        match self {
            ValueHolder::Prime(_) => true,
            _ => false,
        }
    }

    pub fn value(&self) -> &T {
        match self {
            ValueHolder::Value(v) => v,
            ValueHolder::Prime(box ValueHolder::Value(v)) => v,
            _ => panic!("not a prime"),
        }
    }

    /// Consumes a `Box<Value>` or `Box<Tombstone>`, returning a `Prime`
    pub fn to_prime(boxed: Box<ValueHolder<T>>) -> ValueHolder<T> {
        match boxed {
            box ValueHolder::Value(_) | box ValueHolder::Tombstone => ValueHolder::Prime(boxed),
            _ => panic!("already a prime"),
        }
    }

    /// Consumes a `Prime`, returning a `Box<Value>` or `Box<Tombstone>`
    pub fn unwrap_prime(val: ValueHolder<T>) -> Box<ValueHolder<T>> {
        match val {
            ValueHolder::Prime(boxed) => boxed,
            _ => panic!("not a prime"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyHolder, ValueHolder, ValueHolder::Value, ValueHolder::Tombstone, ValueHolder::Prime};

    #[test]
    fn test_keyholder_key_eq() {
        let k1 = KeyHolder::Key(String::from("abc"));
        let k2 = KeyHolder::Key(String::from("abc"));
        assert_eq!(k1, k2);
        println!("{:?}", k1);
    }

    #[test]
    fn test_keyholder_is_tombstone() {
        let k1 = KeyHolder::Key(String::from("abc"));
        assert!(!k1.is_tombstone());
        assert!(KeyHolder::<&str>::Tombstone.is_tombstone());
    }

    #[test]
    fn test_keyholder_eq() {
        let k1 = KeyHolder::Key("abc");
        assert_ne!(k1, KeyHolder::Tombstone);
        let k2 = KeyHolder::<i32>::Tombstone;
        assert_eq!(KeyHolder::Tombstone, k2);
    }

    #[test]
    fn test_valueholder_prime() {
        assert!(!Value(1).is_prime());
        assert!(Prime(Box::new(Value("abc"))).is_prime());
        assert!(!ValueHolder::<usize>::Tombstone.is_prime());
        assert!(Prime::<usize>(Box::new(Tombstone)).is_prime());
    }

    #[test]
    fn test_prime() {
        let v1 = ValueHolder::to_prime(Box::new(ValueHolder::Value(42u64)));
        assert!(v1.is_prime());
        assert!(!v1.is_tombstone());

        let boxed1 = ValueHolder::unwrap_prime(v1);
        assert!(!(*boxed1).is_prime());
        assert!(!(*boxed1).is_tombstone());

        let v2 = ValueHolder::<usize>::to_prime(Box::new(Tombstone));
        assert!(v2.is_prime());
        assert!(v2.is_tombstone());

        let boxed2 = ValueHolder::unwrap_prime(v2);
        assert!(!(*boxed2).is_prime());
        assert!((*boxed2).is_tombstone());
    }

    #[test]
    fn test_valueholder_tombstone() {
        assert!(!Value(1).is_tombstone());
        assert!(!Prime(Box::new(Value("abc"))).is_tombstone());
        assert!(Tombstone::<usize>.is_tombstone());
        assert!(Prime::<usize>(Box::new(Tombstone)).is_tombstone());
    }
}