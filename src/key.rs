use std::hash::Hash;

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

#[cfg(test)]
mod tests {
    use super::KeyHolder;

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
}