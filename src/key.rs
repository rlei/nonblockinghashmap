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
    PrimeValue(T),
    Tombstone,
    PrimeTombstone,
}

impl<T: PartialEq> ValueHolder<T> {
    pub fn is_tombstone(&self) -> bool {
        *self == ValueHolder::Tombstone || *self == ValueHolder::PrimeTombstone
    }

    pub fn is_prime(&self) -> bool {
        match self {
            ValueHolder::PrimeValue(_) => true,
            ValueHolder::PrimeTombstone => true,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{KeyHolder, ValueHolder};

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
        assert!(!ValueHolder::Value(1).is_prime());
        assert!(ValueHolder::PrimeValue("abc").is_prime());
        assert!(!ValueHolder::<usize>::Tombstone.is_prime());
        assert!(ValueHolder::<usize>::PrimeTombstone.is_prime());
    }

    #[test]
    fn test_valueholder_tombstone() {
        assert!(!ValueHolder::Value(1).is_tombstone());
        assert!(!ValueHolder::PrimeValue("abc").is_tombstone());
        assert!(ValueHolder::<usize>::Tombstone.is_tombstone());
        assert!(ValueHolder::<usize>::PrimeTombstone.is_tombstone());
    }
}