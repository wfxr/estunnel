use crate::common::Result;
use reqwest::{Client, Response};
use serde::de::{self, MapAccess};
use serde_derive::*;
use serde_json::{self, value::RawValue, Value};
use std::collections::HashMap;
use std::fmt;
use std::result;

pub type Source = Box<RawValue>;

#[derive(Serialize, Deserialize)]
pub struct ScrollResponse {
    pub _scroll_id: String,
    pub took:       u32,
    pub hits:       Hits,
}

#[derive(Serialize, Deserialize)]
pub struct Hits {
    #[serde(deserialize_with = "parse_total")]
    pub total:     u64,
    pub max_score: f64,
    pub hits:      Vec<Hit>,
}

#[derive(Serialize, Deserialize)]
pub struct Hit {
    pub _source: Source,
}

fn parse_total<'de, D>(deserializer: D) -> result::Result<u64, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct TotalVisitor;

    impl<'de> de::Visitor<'de> for TotalVisitor {
        type Value = u64;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("u64 or map contains u64 value")
        }

        fn visit_u64<E>(self, v: u64) -> result::Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            Ok(v)
        }

        fn visit_map<A>(self, access: A) -> result::Result<Self::Value, A::Error>
        where
            A: MapAccess<'de>,
        {
            let map: HashMap<&str, Value> =
                serde::Deserialize::deserialize(de::value::MapAccessDeserializer::new(access))?;
            let v = map
                .get("value")
                .ok_or_else(|| de::Error::missing_field("value"))?
                .as_u64()
                .expect("not a valid u64");
            Ok(v)
        }
    }
    deserializer.deserialize_any(TotalVisitor)
}

pub fn parse_response(mut res: Response) -> Result<(Vec<Box<RawValue>>, String, u64)> {
    let res = res.text()?;
    // serde_json has bad performance on reader. So we first read body into a string.
    // See: https://github.com/serde-rs/json/issues/160
    let res: ScrollResponse = serde_json::from_str(&res)?;
    let docs = res.hits.hits.into_iter().map(|hit| hit._source).collect();
    Ok((docs, res._scroll_id, res.hits.total))
}

pub fn request_elastic(
    client: &Client,
    url: &str,
    query: &Value,
    user: &str,
    pass: &Option<String>,
    params: Option<Vec<(&str, String)>>,
) -> Result<Response> {
    let res = client.post(url).basic_auth(user, pass.clone()).json(query);

    let res = match params {
        Some(params) => res.query(&params),
        _ => res,
    };

    let res = res.send()?.error_for_status()?;
    Ok(res)
}
