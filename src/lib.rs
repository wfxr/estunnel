#[macro_use]
extern crate serde_derive;
extern crate serde_json;

#[derive(Serialize, Deserialize, Debug)]
pub struct ScrollResponse {
    pub _scroll_id: String,
    pub took: i32,
    pub hits: Hits,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Hits {
    pub total: i64,
    pub max_score: f64,
    pub hits: Vec<Hit>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Hit {
    pub _source: serde_json::Value
}
