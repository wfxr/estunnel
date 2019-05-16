extern crate estunnel;

extern crate serde_derive;
#[macro_use]
extern crate serde_json;

use estunnel::ScrollResponse;

use crossbeam::crossbeam_channel;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::StatusCode;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use structopt::StructOpt;

const CHANNEL_CAPACITY: usize = 10000;

fn main() -> Result<(), Box<std::error::Error>> {
    let opt = Opt::from_args();
    let host = opt.host;
    let index = opt.index;
    let slice = opt.slice;
    let batch: Option<u32> = opt.batch;
    let scroll_ttl = opt.scroll_ttl;

    let query = BufReader::new(File::open(opt.query)?);
    let query: serde_json::Value = serde_json::from_reader(query)?;

    let (tx, rx) = crossbeam_channel::bounded(CHANNEL_CAPACITY);

    let mpb = Arc::new(MultiProgress::new());
    let mut producer_threads = vec![];
    for slice_id in 0..slice {
        let tx = tx.clone();
        let mut query = query.clone();
        let host = host.clone();
        let index = index.clone();
        let scroll_ttl = scroll_ttl.clone();

        let mpb = mpb.clone();
        let pb = mpb.add(ProgressBar::new_spinner());
        let style = ProgressStyle::default_bar()
            .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos:>7}/{len:7} {msg}")
            .progress_chars("##-");
        pb.set_style(style);
        pb.set_message(&format!("Slice {}", slice_id));

        producer_threads.push(thread::spawn(move || {
            let client = reqwest::Client::new();
            if slice > 1 {
                let obj = query.as_object_mut().unwrap();
                obj.insert(
                    "slice".into(),
                    json!({
                        "id": slice_id,
                        "max": slice
                    }),
                );
                query = json!(obj);
            }

            let mut params = vec![("scroll", "1m".to_owned())];
            if let Some(batch) = batch {
                params.push(("size", batch.to_string()))
            }
            let mut res = client
                .post(&format!("{}/{}/_search", &host, &index))
                .query(&params)
                .json(&query)
                .send()
                .expect("error sending request");

            if res.status() != 200 {
                eprintln!(
                    "error query es. status={}, content={}",
                    res.status(),
                    res.text().unwrap()
                );
                return;
            }

            let res: ScrollResponse = res.json().expect("error parsing response");

            pb.set_length(res.hits.total as u64);
            pb.inc(res.hits.hits.len() as u64);

            let mut scroll_id = res._scroll_id.clone();
            let mut finished = res.hits.hits.is_empty();
            tx.send(Box::new(res)).expect("error sending result to channel");

            while !finished {
                let mut res = client
                    .post(&format!("{}/_search/scroll", &host))
                    .json(&json!({
                        "scroll": scroll_ttl,
                        "scroll_id": scroll_id,
                    }))
                    .send()
                    .expect("error sending request");

                if res.status() != StatusCode::OK {
                    eprintln!("error query es: {:?}", res);
                    return;
                }
                let res: ScrollResponse = res.json().expect("error parsing response");
                pb.inc(res.hits.hits.len() as u64);
                scroll_id = res._scroll_id.clone();
                finished = res.hits.hits.is_empty();
                tx.send(Box::new(res)).expect("error sending result to channel");
            }

            pb.finish_with_message(&format!("Slice {} finished", slice_id))
        }));
    }

    thread::spawn(|| {
        for th in producer_threads {
            th.join().expect("error joining");
        }
        drop(tx);
    });

    let output = opt.output;
    let consumer_thread = thread::spawn(move || {
        let mut output = BufWriter::new(match output {
            Some(output) => Box::new(File::create(output).unwrap()) as Box<Write>,
            None => Box::new(std::io::stdout()),
        });
        for res in rx.iter() {
            for hit in &res.hits.hits {
                writeln!(&mut output, "{}", hit._source).unwrap();
            }
        }
    });

    mpb.join_and_clear().unwrap();
    consumer_thread.join().unwrap();
    Ok(())
}

#[derive(StructOpt, Debug)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
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

    #[structopt(short = "o", long = "output")]
    output: Option<PathBuf>,

    #[structopt(long = "scroll-ttl", default_value = "1m")]
    scroll_ttl: String,
}
