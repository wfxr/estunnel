extern crate serde_derive;
#[macro_use]
extern crate serde_json;

extern crate elastic_tunnel_rs;

use elastic_tunnel_rs::ScrollResponse;

use crossbeam::crossbeam_channel;
use std::thread;
use serde_json::Value;
use reqwest::StatusCode;
use std::io::BufReader;
use structopt::StructOpt;
use std::path::PathBuf;
use std::fs::File;

fn main() -> Result<(), Box<std::error::Error>> {
    let opt = Opt::from_args();
    let host = opt.host;
    let index = opt.index;
    let slice = opt.slice;
    let batch: Option<u32> = opt.batch;

    let query = BufReader::new(File::open(opt.query)?);
    let query: serde_json::Value = serde_json::from_reader(query)?;

    let (s, r) = crossbeam_channel::bounded(0);

    let mut children = vec![];
    for slice_id in 0..slice {
        let s = s.clone();
        let mut query = query.clone();
        let host = host.clone();
        let index = index.clone();
        children.push(thread::spawn(move || {
            let client = reqwest::Client::new();

            if slice > 1 {
                let obj = query.as_object_mut().unwrap();
                obj.insert("slice".into(), json!({
                    "id": slice_id,
                    "max": slice
                }));
                query = json!(obj);
            }

            let mut params = vec![("scroll", "1m".to_owned())];
            match batch {
                Some(batch) => params.push(("size", batch.to_string())),
                None => (),
            }
            let mut res = client
                .post(&format!("{}/{}/_search", &host, &index))
                .query(&params)
                .json(&query)
                .send().expect("error sending request");

            if res.status() != 200 {
                eprintln!("error query es. status={}, content={}", res.status(), res.text().unwrap());
                return;
            }

            let res: ScrollResponse = res
                .json().expect("error parsing response");

            let mut scroll_id = res._scroll_id.clone();
            let mut finished = res.hits.hits.is_empty();
            s.send(res).expect("error sending result to channel");

            while !finished {
                let mut res = reqwest::Client::new()
                    .post("http://47.93.125.169:9200/_search/scroll")
                    .json(&json!({
                        "scroll": "1m",
                        "scroll_id": scroll_id,
                    }))
                    .send().expect("error sending request");

                if res.status() != StatusCode::OK {
                    eprintln!("error query es: {:?}", res);
                    return;
                }
                let res: ScrollResponse = res
                    .json().expect("error parsing response");
                scroll_id = res._scroll_id.clone();
                finished = res.hits.hits.is_empty();
                s.send(res).expect("error sending result to channel");
            }
        }));
    }

    thread::spawn(|| {
        for child in children {
            child.join().expect("error joining");
        }
        drop(s);
    });

    for res in r {
        for hit in res.hits.hits {
            println!("{}", json!(hit._source));
        }
    }
    Ok(())
}


#[derive(StructOpt, Debug)]
#[structopt(name = "basic")]
struct Opt {
    #[structopt(short = "h", long = "host", default_value = "http://localhost:9200")]
    host: String,

    #[structopt(short = "i", long = "index")]
    index: String,

    #[structopt(short = "q", long = "query")]
    query: PathBuf,

    #[structopt(short = "s", long = "slice", default_value = "1")]
    slice: u32,

    #[structopt(short = "b", long = "batch")]
    batch: Option<u32>,
}
