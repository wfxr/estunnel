#[macro_use]
extern crate serde_derive;
extern crate serde_json;

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
