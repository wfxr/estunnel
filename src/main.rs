extern crate estunnel;

extern crate serde_derive;
#[macro_use]
extern crate serde_json;

use estunnel::ScrollResponse;

use crossbeam::crossbeam_channel;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Response;
use std::cmp::{max, min};
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
        let pb = mpb.add(ProgressBar::new(1));
        let style = ProgressStyle::default_bar()
            .template("{prefix} [{elapsed_precise}] {bar:60.cyan/blue} {msg}")
            .progress_chars("##-");
        pb.set_style(style);
        pb.set_prefix(&format!("Slice-{}", slice_id));
        pb.set_message("Starting...");

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
            let res = client
                .post(&format!("{}/{}/_search", &host, &index))
                .query(&params)
                .json(&query)
                .send()
                .expect("error sending request");

            let (docs, mut scroll_id, total) = parse_response(res).expect("error parsing response");

            let style = ProgressStyle::default_bar()
                .template("{prefix} [{elapsed_precise}] {bar:60.cyan/blue} {msg} {pos:>7}/{len:7} (ETA {eta_precise})")
                .progress_chars("##-");
            pb.set_message("Running...");
            pb.set_style(style);
            pb.set_length(total);
            pb.set_draw_delta(max(1, min(10000, total / 1000)));
            pb.inc(docs.len() as u64);

            let mut finished = docs.is_empty();
            tx.send(Box::new(docs)).expect("error sending result to channel");

            while !finished {
                let res = client
                    .post(&format!("{}/_search/scroll", &host))
                    .json(&json!({
                        "scroll": scroll_ttl,
                        "scroll_id": scroll_id,
                    }))
                    .send()
                    .expect("error sending request");

                let (docs, new_scroll_id, total) = parse_response(res).expect("error parsing response");

                scroll_id = new_scroll_id;
                pb.set_length(total);
                pb.inc(docs.len() as u64);
                finished = docs.is_empty();
                tx.send(Box::new(docs)).expect("error sending result to channel");
            }

            pb.finish_with_message("Finished.")
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
        for docs in rx.iter() {
            for doc in docs.iter() {
                writeln!(&mut output, "{}", doc).unwrap();
            }
        }
    });

    mpb.join_and_clear().unwrap();
    consumer_thread.join().unwrap();
    Ok(())
}

fn parse_response(mut res: Response) -> Result<(Vec<String>, String, u64), Box<std::error::Error>> {
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
