use serde::{
    de::{self, DeserializeSeed, IntoDeserializer, MapAccess, SeqAccess, Visitor},
    forward_to_deserialize_any, Deserialize,
};
use std::collections::{BTreeMap, VecDeque};

mod error;

pub use error::Error;

// Copied from serde_urlencoded and modified
macro_rules! forward_parsed_value {
    ($($ty:ident => $method:ident,)*) => {
        $(
            fn $method<V>(self, visitor: V) -> Result<V::Value, Self::Error>
                where V: de::Visitor<'de>
            {
                match self.next_unit()?.as_str().parse::<$ty>() {
                    Ok(val) => val.into_deserializer().$method(visitor),
                    Err(e) => Err(de::Error::custom(e))
                }
            }
        )*
    }
}

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

    fn next_unit(&mut self) -> Result<String, Error> {
        let values = self.current_values()?;
        let value = values.pop_front().ok_or(Error::MissingValue)?;

        if values.is_empty() {
            let key = self.current_key()?;
            self.key_values
                .remove(&key)
                .ok_or_else(|| Error::RemoveKeyFailed(key))?;
            self.in_sequence = false;
        }

        Ok(value)
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
        let value = self.next_unit()?;
        visitor.visit_string(value)
    }

    fn deserialize_option<V>(self, visitor: V) -> Result<V::Value, Self::Error>
    where
        V: Visitor<'de>,
    {
        if !self.in_map {
            return Err(Error::ForbiddenTopLevelOption);
        }
        visitor.visit_some(self)
    }

    fn deserialize_enum<V>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        let value = self.next_unit()?;
        visitor.visit_enum(EnumAccess(value))
    }

    forward_to_deserialize_any! {
        char str
        bytes byte_buf unit unit_struct newtype_struct tuple
        tuple_struct ignored_any
    }

    forward_parsed_value! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}

struct EnumAccess(String);

impl<'de, 'a> de::EnumAccess<'de> for EnumAccess {
    type Error = Error;
    type Variant = UnitOnlyVariantAccess;

    fn variant_seed<V>(self, seed: V) -> Result<(V::Value, Self::Variant), Self::Error>
    where
        V: de::DeserializeSeed<'de>,
    {
        let variant = seed.deserialize::<de::value::StringDeserializer<Self::Error>>(
            self.0.into_deserializer(),
        )?;
        Ok((variant, UnitOnlyVariantAccess))
    }
}

struct UnitOnlyVariantAccess;

impl<'de, 'a> de::VariantAccess<'de> for UnitOnlyVariantAccess {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Self::Error> {
        Ok(())
    }

    fn newtype_variant_seed<T>(self, _seed: T) -> Result<T::Value, Self::Error>
    where
        T: de::DeserializeSeed<'de>,
    {
        Err(Error::ExpectedUnitVariant)
    }

    fn tuple_variant<V>(self, _len: usize, _visitor: V) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::ExpectedUnitVariant)
    }

    fn struct_variant<V>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error>
    where
        V: de::Visitor<'de>,
    {
        Err(Error::ExpectedUnitVariant)
    }

    forward_parsed_value! {
        bool => deserialize_bool,
        u8 => deserialize_u8,
        u16 => deserialize_u16,
        u32 => deserialize_u32,
        u64 => deserialize_u64,
        i8 => deserialize_i8,
        i16 => deserialize_i16,
        i32 => deserialize_i32,
        i64 => deserialize_i64,
        f32 => deserialize_f32,
        f64 => deserialize_f64,
    }
}

#[cfg(test)]
mod test {
    use std::cmp::{Eq, Ord, PartialEq, PartialOrd};

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

        assert_eq!(ids.id, string_vec(&[1, 2, 3]));
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

    #[test]
    fn option_field_deserialize_with() {
        #[derive(Debug, Deserialize)]
        pub struct Opt {
            #[serde(default, deserialize_with = "option_query_vec")]
            x: Option<Vec<u64>>,
        }

        fn option_query_vec<'de, D, T>(deserializer: D) -> Result<Option<Vec<T>>, D::Error>
        where
            D: serde::Deserializer<'de>,
            T: std::str::FromStr,
        {
            let vec: Vec<String> = Deserialize::deserialize(deserializer)?;
            if !vec.is_empty() {
                Ok(Some(
                    vec.into_iter()
                        .map(|s| T::from_str(&s).map_err(|_| ()).unwrap())
                        .collect(),
                ))
            } else {
                Ok(None)
            }
        }

        let q = "";
        let v = from_str::<Opt>(q).unwrap();
        assert_eq!(v.x, None);

        let q = "x=1&x=2";
        let v = from_str::<Opt>(q).unwrap();
        assert_eq!(v.x, Some(vec![1, 2]));
    }

    #[test]
    fn option_string() {
        #[derive(Debug, PartialEq, Deserialize)]
        pub struct Example {
            x: Option<String>,
            y: String,
            z: Option<String>,
        }

        let data = vec![
            (
                "y=5",
                Example {
                    x: None,
                    y: "5".into(),
                    z: None,
                },
            ),
            (
                "y=5&x=2",
                Example {
                    x: Some("2".into()),
                    y: "5".into(),
                    z: None,
                },
            ),
            (
                "y=1&z=2",
                Example {
                    x: None,
                    y: "1".into(),
                    z: Some("2".into()),
                },
            ),
            (
                "x=hello&y=world&z=wow",
                Example {
                    x: Some("hello".into()),
                    y: "world".into(),
                    z: Some("wow".into()),
                },
            ),
        ];

        for (query, expected) in data {
            assert_eq!(from_str::<Example>(query).unwrap(), expected);
        }

        // Missing `y`.
        from_str::<Example>("").unwrap_err();
    }

    #[test]
    fn array_and_number() {
        #[derive(Debug, Deserialize)]
        pub struct Query {
            id: Vec<String>,
            foo: u32,
        }

        let q = "id=1&id=2&foo=3";
        let ids: Query = from_str(q).unwrap();

        assert_eq!(ids.id, string_vec(&[1, 2]));
        assert_eq!(ids.foo, 3);
    }
    #[test]
    fn simple_enum() {
        #[derive(Debug, Deserialize)]
        pub struct Query {
            id: Vec<MyEnum>,
            foo: MyEnum,
        }

        #[derive(Debug, Eq, Ord, PartialEq, PartialOrd, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum MyEnum {
            A,
            B,
            C,
        }

        let q = "id=a&id=b&id=c&id=b&foo=c";
        let ids: Query = from_str(q).unwrap();
        assert_eq!(ids.id, vec![MyEnum::A, MyEnum::B, MyEnum::C, MyEnum::B]);
        assert_eq!(ids.foo, MyEnum::C);
    }
}
