use reqwest::Response;
use serde_derive::*;
use serde_json;

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

pub fn parse_response(mut res: Response) -> Result<(Vec<String>, String, u64), Box<std::error::Error>> {
    if res.status() != 200 {
        return Err(format!("error query es. status={}, content={}", res.status(), res.text()?).into());
    }
    // serde_json has bad performance on reader. So we first read body into a string.
    // See: https://github.com/serde-rs/json/issues/160
    let res = res.text()?;
    let res: ScrollResponse = serde_json::from_str(&res)?;
    let docs = res.hits.hits.iter().map(|hit| hit._source.to_string()).collect();
    Ok((docs, res._scroll_id, res.hits.total))
}
