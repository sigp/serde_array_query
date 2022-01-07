use serde::{
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    forward_to_deserialize_any, Deserialize,
};
use std::collections::{BTreeMap, VecDeque};

mod error;

pub use error::Error;

#[derive(Debug)]
pub struct Deserializer {
    key_values: BTreeMap<String, VecDeque<String>>,
    in_map: bool,
    in_sequence: bool,
}

impl Deserializer {
    pub fn from_key_values(input: Vec<(String, String)>) -> Self {
        let mut key_values = BTreeMap::<_, VecDeque<String>>::new();

        for (k, v) in input {
            key_values.entry(k).or_default().push_back(v);
        }

        Self {
            key_values,
            in_map: false,
            in_sequence: false,
        }
    }

    /// Return the next key to be read by the visitor.
    fn current_key(&self) -> Result<String, Error> {
        // TODO: could maybe avoid the clone here if we fiddle with deserializer lifetimes
        self.key_values
            .keys()
            .next()
            .cloned()
            .ok_or(Error::MissingKey)
    }

    fn current_values(&mut self) -> Result<&mut VecDeque<String>, Error> {
        self.key_values
            .values_mut()
            .next()
            .ok_or(Error::MissingValues)
    }
}

#[cfg(feature = "from_str")]
pub fn from_str<'a, T>(s: &str) -> Result<T, Error>
where
    T: Deserialize<'a>,
{
    from_key_values(serde_urlencoded::from_str(s)?)
}

pub fn from_key_values<'a, T>(key_values: Vec<(String, String)>) -> Result<T, Error>
where
    T: Deserialize<'a>,
{
    let mut deserializer = Deserializer::from_key_values(key_values);
    let t = T::deserialize(&mut deserializer)?;
    if deserializer.key_values.is_empty() {
        Ok(t)
    } else {
        Err(Error::TrailingValues)
    }
}

impl<'de, 'a> MapAccess<'de> for &'a mut Deserializer {
    type Error = Error;

    fn next_key_seed<K>(&mut self, seed: K) -> Result<Option<K::Value>, Error>
    where
        K: DeserializeSeed<'de>,
    {
        if self.key_values.is_empty() {
            Ok(None)
        } else {
            seed.deserialize(&mut **self).map(Some)
        }
    }

    fn next_value_seed<V>(&mut self, seed: V) -> Result<V::Value, Error>
    where
        V: DeserializeSeed<'de>,
    {
        seed.deserialize(&mut **self)
    }
}

impl<'de, 'a> SeqAccess<'de> for &'a mut Deserializer {
    type Error = Error;

    fn next_element_seed<T>(&mut self, seed: T) -> Result<Option<T::Value>, Error>
    where
        T: DeserializeSeed<'de>,
    {
        if !self.in_sequence {
            Ok(None)
        } else {
            seed.deserialize(&mut **self).map(Some)
        }
    }
}

impl<'de, 'a> de::Deserializer<'de> for &'a mut Deserializer {
    type Error = Error;

    fn deserialize_any<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_string(visitor)
    }

    fn deserialize_struct<V>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        self.deserialize_map(visitor)
    }

    fn deserialize_map<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if self.in_map {
            return Err(Error::ForbiddenNestedMap);
        }

        self.in_map = true;
        let result = visitor.visit_map(&mut self)?;
        self.in_map = false;

        Ok(result)
    }

    fn deserialize_identifier<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let key = self.current_key()?;
        visitor.visit_string(key)
    }

    fn deserialize_seq<V>(mut self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        // Disallow sequences within sequences for simplicity.
        if self.in_sequence {
            return Err(Error::ForbiddenNestedSequence);
        }

        self.in_sequence = true;
        let result = visitor.visit_seq(&mut self)?;

        // The `in_sequence` bool should be switched off after reading all elements.
        if self.in_sequence {
            return Err(Error::SequenceNotConsumed);
        }

        Ok(result)
    }

    fn deserialize_string<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        let values = self.current_values()?;
        let value = values.pop_front().ok_or(Error::MissingValue)?;

        if values.is_empty() {
            let key = self.current_key()?;
            self.key_values
                .remove(&key)
                .ok_or_else(|| Error::RemoveKeyFailed(key))?;
            self.in_sequence = false;
        }
        visitor.visit_string(value)
    }

    // TODO: could support integer types by using a macro to generate impls that use `FromStr`
    forward_to_deserialize_any! {
        bool i8 i16 i32 i64 i128 u8 u16 u32 u64 u128 f32 f64 char str
        bytes byte_buf option unit unit_struct newtype_struct tuple
        tuple_struct enum ignored_any
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde::Deserialize;

    fn string_vec(v: &[u64]) -> Vec<String> {
        v.into_iter().map(|v| v.to_string()).collect()
    }

    #[test]
    fn single_array() {
        #[derive(Debug, Deserialize)]
        pub struct IdVec {
            id: Vec<String>,
        }

        let q = "id=1&id=2&id=3";
        let ids: IdVec = from_str(q).unwrap();

        assert_eq!(ids.id, string_vec(&[1, 2, 3]),);
    }

    #[test]
    fn no_nested_map() {
        #[derive(Debug, Deserialize)]
        pub struct L1 {
            #[allow(dead_code)]
            y: Vec<String>,
        }

        #[derive(Debug, Deserialize)]
        pub struct L2 {
            #[allow(dead_code)]
            x: Vec<L1>,
        }

        let q = "x=y=1&y=2";
        let err = from_str::<L2>(q).unwrap_err();

        assert!(matches!(err, Error::ForbiddenNestedMap));
    }

    #[test]
    fn nested_map_from_str() {
        #[derive(Debug, Deserialize)]
        #[serde(from = "String")]
        pub struct QueryVec {
            values: Vec<String>,
        }

        impl From<String> for QueryVec {
            fn from(s: String) -> Self {
                Self {
                    values: s.split(',').map(str::to_string).collect(),
                }
            }
        }

        fn flat_query_vec<'de, D>(deserializer: D) -> Result<QueryVec, D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            let vec: Vec<QueryVec> = Deserialize::deserialize(deserializer)?;
            Ok(QueryVec::from(vec))
        }

        impl From<Vec<QueryVec>> for QueryVec {
            fn from(vecs: Vec<QueryVec>) -> Self {
                Self {
                    values: vecs.into_iter().flat_map(|qv| qv.values).collect(),
                }
            }
        }

        #[derive(Debug, Deserialize)]
        pub struct Example {
            x: Vec<QueryVec>,
        }

        #[derive(Debug, Deserialize)]
        pub struct FlatExample {
            #[serde(deserialize_with = "flat_query_vec")]
            x: QueryVec,
        }

        let q = "x=1,2,3&x=4,5,6,7,8&x=9";

        let v = from_str::<Example>(q).unwrap();

        assert_eq!(v.x.len(), 3);
        assert_eq!(v.x[0].values, string_vec(&[1, 2, 3]));
        assert_eq!(v.x[1].values, string_vec(&[4, 5, 6, 7, 8]));
        assert_eq!(v.x[2].values, string_vec(&[9]));

        let flat = from_str::<FlatExample>(q).unwrap();
        assert_eq!(flat.x.values, string_vec(&[1, 2, 3, 4, 5, 6, 7, 8, 9]));
    }
}
