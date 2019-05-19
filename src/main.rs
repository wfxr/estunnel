use estunnel::ScrollResponse;

use crossbeam::crossbeam_channel::{self, Sender};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use reqwest::Response;
use serde_json::json;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use structopt::StructOpt;
use threadpool::ThreadPool;

const CHANNEL_CAPACITY: usize = 10;

struct Package {
    pub slice_id: u32,
    pub total:    u64,
    pub docs:     Vec<String>,
}

struct MPB {
    pub mpb: Arc<MultiProgress>,
    pbs:     HashMap<u32, ProgressBar>,
    size:    usize,
    joined:  bool,
}

impl MPB {
    fn new(size: usize) -> MPB {
        MPB {
            mpb: Arc::new(MultiProgress::new()),
            pbs: HashMap::new(),
            size,
            joined: false,
        }
    }
    fn inc(&mut self, id: u32, delta: u64, total: u64) {
        if self.pbs.contains_key(&id) {
            self.pbs[&id].set_length(total);
            self.pbs[&id].inc(delta)
        } else {
            let pb = self.mpb.add(ProgressBar::new(total));
            let style = ProgressStyle::default_bar()
                .template("{prefix} [{elapsed_precise}] {bar:60.cyan/blue} {msg} {pos:>7}/{len:7} (ETA {eta_precise})")
                .progress_chars("##-");
            pb.set_style(style);
            pb.set_prefix(&format!("Slice-{}", id));
            pb.set_message("Running...");
            pb.inc(delta);
            self.pbs.insert(id, pb);
        }
        if !self.joined && self.pbs.len() >= self.size {
            self.joined = true;
            let mpb = self.mpb.clone();
            thread::spawn(move || {
                mpb.join_and_clear().unwrap();
            });
        }
    }

    fn finish_with_message(&self, msg: &str) {
        for pb in self.pbs.values() {
            pb.finish_with_message(msg)
        }
    }
}

fn main() -> Result<(), Box<std::error::Error>> {
    let opt = Opt::from_args();
    let host = opt.host;
    let index = opt.index;
    let slice = opt.slice;
    let batch: Option<u32> = opt.batch;
    let scroll_ttl = opt.scroll_ttl;
    let user = opt.user;
    let pass = match &user {
        Some(user) => {
            let prompt = format!("Enter host password for user {}: ", user.clone());
            Some(rpassword::read_password_from_tty(Some(&prompt)).unwrap())
        }
        None => None,
    };
    let user = user.unwrap_or("estunnel".to_owned());

    let query = BufReader::new(File::open(opt.query)?);
    let query: serde_json::Value = serde_json::from_reader(query)?;

    let (tx, rx) = crossbeam_channel::bounded(CHANNEL_CAPACITY);

    let io_pool = ThreadPool::new(slice as usize);
    let compute_pool = ThreadPool::new(num_cpus::get());
    for slice_id in 0..slice {
        let tx = tx.clone();
        let mut query = query.clone();
        let host = host.clone();
        let index = index.clone();
        let scroll_ttl = scroll_ttl.clone();
        let user = user.clone();
        let pass = pass.clone();

        let compute_pool = compute_pool.clone();
        io_pool.execute(move || {
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
                .basic_auth(user.clone(), pass.clone())
                .query(&params)
                .json(&query)
                .send()
                .expect("error sending request");

            let tx1: Sender<Box<Package>> = tx.clone();
            let pool = compute_pool.clone();
            let (mut scroll_id, mut finished) = parse_response(slice_id, res, pool, move |package| {
                tx1.send(package).expect("error sending result to channel");
            })
            .expect("error parsing response");

            while !finished {
                let res = client
                    .post(&format!("{}/_search/scroll", &host))
                    .basic_auth(user.clone(), pass.clone())
                    .json(&json!({
                        "scroll": scroll_ttl,
                        "scroll_id": scroll_id,
                    }))
                    .send()
                    .expect("error sending request");

                let tx1 = tx.clone();
                let pool = compute_pool.clone();
                let (new_scroll_id, new_finished) = parse_response(slice_id, res, pool, move |package| {
                    tx1.send(package).expect("error sending result to channel");
                })
                .expect("error parsing response");

                scroll_id = new_scroll_id;
                finished = new_finished;
            }
        });
    }

    thread::spawn(move || {
        io_pool.join();
        drop(tx);
    });

    let output = opt.output;
    let consumer_thread = thread::spawn(move || {
        let mut output = BufWriter::new(match output {
            Some(output) => Box::new(File::create(output).unwrap()) as Box<Write>,
            None => Box::new(std::io::stdout()),
        });

        let mut mpb = MPB::new(slice as usize);

        for pack in rx.iter() {
            mpb.inc(pack.slice_id, pack.docs.len() as u64, pack.total);
            for doc in pack.docs.iter() {
                writeln!(&mut output, "{}", doc).unwrap();
            }
        }
        mpb.finish_with_message("Finished.");
    });

    consumer_thread.join().unwrap();
    Ok(())
}

fn parse_response<F: Fn(Box<Package>) -> ()>(
    slice_id: u32,
    mut res: Response,
    pool: ThreadPool,
    f: F,
) -> Result<(String, bool), Box<std::error::Error>>
where
    F: std::marker::Send + 'static,
{
    if res.status() != 200 {
        return Err(format!("error query es. status={}, content={}", res.status(), res.text()?).into());
    }
    // serde_json has bad performance on reader. So we first read body into a string.
    // See: https://github.com/serde-rs/json/issues/160
    let res = res.text()?;
    let scroll_id = extract_scroll_id(&res).unwrap().to_owned();
    let is_finish = extract_scroll_finished(&res).unwrap();

    pool.execute(move || {
        let res: ScrollResponse = serde_json::from_str(&res).unwrap();
        let docs = res.hits.hits.iter().map(|hit| hit._source.to_string()).collect();
        f(Box::new(Package {
            slice_id,
            total: res.hits.total,
            docs,
        }))
    });

    Ok((scroll_id, is_finish))
}

fn extract_scroll_id(s: &str) -> Result<&str, ()> {
    const PATTERN: &str = r#""_scroll_id":""#;
    let start = s.find(PATTERN).unwrap() + PATTERN.len();
    let scroll_id = &s[start..];
    let end = scroll_id.find("\"").unwrap();
    let scroll_id = &scroll_id[..end];
    Ok(scroll_id)
}

fn extract_scroll_finished(s: &str) -> Result<bool, ()> {
    const PATTERN: &str = r#""_index":""#;
    Ok(!s.find(PATTERN).is_some())
}

#[derive(StructOpt, Debug)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
struct Opt {
    #[structopt(short = "h", long = "host", default_value = "http://localhost:9200")]
    host: String,

    #[structopt(short = "u", long = "user")]
    user: Option<String>,

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
