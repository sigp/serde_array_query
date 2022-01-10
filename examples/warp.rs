#![allow(dead_code)]

use serde::{de, Deserialize};
use serde_array_query::from_key_values;
use std::str::FromStr;
use warp::{http::Response, Filter};

#[derive(Clone, PartialEq, Debug, Deserialize)]
#[serde(try_from = "String", bound = "T: FromStr")]
pub struct QueryVec<T: FromStr> {
    values: Vec<T>,
}

fn query_vec<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: FromStr,
{
    let vec: Vec<QueryVec<T>> = Deserialize::deserialize(deserializer)?;
    QueryVec::try_into(QueryVec::from(vec)).map_err(de::Error::custom)
}

impl<T: FromStr> From<Vec<QueryVec<T>>> for QueryVec<T> {
    fn from(vecs: Vec<QueryVec<T>>) -> Self {
        Self {
            values: vecs.into_iter().flat_map(|qv| qv.values).collect(),
        }
    }
}

impl<T: FromStr> TryInto<Vec<T>> for QueryVec<T> {
    type Error = String;

    fn try_into(self) -> Result<Vec<T>, Self::Error> {
        Ok(self.values)
    }
}

impl<T: FromStr> TryFrom<String> for QueryVec<T> {
    type Error = String;

    fn try_from(string: String) -> Result<Self, Self::Error> {
        if string.is_empty() {
            return Ok(Self{ values: vec![]});
        }

        Ok(Self {
            values: string
            .split(',')
            .map(|s| s.parse().map_err(|_| "unable to parse".to_string()))
            .collect::<Result<Vec<T>, String>>()?
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct Example1 {
    #[serde(deserialize_with = "query_vec")]
    key1: Vec<String>,
}

fn example_1_filter(query: Vec<(String, String)>) -> Response<String> {
    let string = from_key_values::<Example1>(query).unwrap();
    Response::builder().body(format!("{:?}", string)).unwrap()
}

#[derive(Debug, Deserialize)]
pub struct Example2 {
    #[serde(deserialize_with = "query_vec")]
    key1: Vec<u64>,
}

fn example_2_filter(query: Vec<(String, String)>) -> Response<String> {
    let string = from_key_values::<Example2>(query).unwrap();
    Response::builder().body(format!("{:?}", string)).unwrap()
}

#[derive(Debug, Deserialize)]
pub struct Example3 {
    #[serde(deserialize_with = "query_vec")]
    key1: Vec<String>,
    #[serde(deserialize_with = "query_vec")]
    key2: Vec<u64>,
}

fn example_3_filter(query: Vec<(String, String)>) -> Response<String> {
    let string = from_key_values::<Example3>(query).unwrap();
    Response::builder().body(format!("{:?}", string)).unwrap()
}

#[tokio::main]
async fn main() {

    // curl "http://localhost:3030/example1?key1=hello,world&key1=foo,bar"
    // demonstrates deserializing duplicate key-value pairs into a Vec<String>
    let example1 = warp::get().and(
        warp::path("example1")
            .and(warp::query())
            .map(example_1_filter),
    );

    // curl "http://localhost:3030/example1?key1=1,2,3key1=42"
    // demonstrates deserializing duplicate key-value pairs into a Vec<u64>
    let example2 = warp::get().and(
        warp::path("example2")
            .and(warp::query())
            .map(example_2_filter),
    );

    // curl "http://localhost:3030/example3?key1=hello&key2=1&key1=world,foo&key2=2,42"
    // demonstrates deserializing multiple duplicate key-value pairs into their corresponding Vecs
    let example3 = warp::get().and(
        warp::path("example3")
            .and(warp::query())
            .map(example_3_filter),
    );

    warp::serve(example1.or(example2).or(example3))
        .run(([127, 0, 0, 1], 3030))
        .await;
}
