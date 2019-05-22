use crate::common::Result;
use reqwest::{Client, Response};
use serde_derive::*;
use serde_json;
use serde_json::Value;

#[derive(Serialize, Deserialize)]
pub struct ScrollResponse {
    pub _scroll_id: String,
    pub took:       u32,
    pub hits:       Hits,
}

#[derive(Serialize, Deserialize)]
pub struct Hits {
    pub total:     u64,
    pub max_score: f64,
    pub hits:      Vec<Hit>,
}

#[derive(Serialize, Deserialize)]
pub struct Hit {
    pub _source: serde_json::Value,
}

pub fn parse_response(mut res: Response) -> Result<(Vec<String>, String, u64)> {
    let res = res.text()?;
    // serde_json has bad performance on reader. So we first read body into a string.
    // See: https://github.com/serde-rs/json/issues/160
    let res: ScrollResponse = serde_json::from_str(&res)?;
    let docs = res.hits.hits.iter().map(|hit| hit._source.to_string()).collect();
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
