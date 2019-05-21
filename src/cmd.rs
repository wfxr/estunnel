use crate::cli::{Opt, Shell, StructOpt};
use crate::common::Result;
use crate::elastic::parse_response;
use crossbeam::crossbeam_channel;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde_json::json;
use std::cmp::{max, min};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

pub fn completion(shell: Shell, output: PathBuf) -> Result<()> {
    Ok(Opt::clap().gen_completions(env!("CARGO_PKG_NAME"), shell, output))
}

pub fn pull(
    host: String,
    user: Option<String>,
    index: String,
    query: PathBuf,
    slice: u32,
    batch: Option<u32>,
    output: PathBuf,
    ttl: String,
) -> Result<()> {
    let pass = match &user {
        Some(user) => {
            let prompt = format!("Enter host password for user {}: ", user.clone());
            Some(rpassword::read_password_from_tty(Some(&prompt)).unwrap())
        }
        None => None,
    };
    let user = user.unwrap_or("estunnel".to_owned());

    let query = BufReader::new(File::open(query)?);
    let query: serde_json::Value = serde_json::from_reader(query)?;

    let (tx, rx) = crossbeam_channel::bounded(slice as usize);

    let mpb = Arc::new(MultiProgress::new());
    let pool = threadpool::ThreadPool::new(slice as usize);
    for slice_id in 0..slice {
        let tx = tx.clone();
        let mut query = query.clone();
        let host = host.clone();
        let index = index.clone();
        let scroll_ttl = ttl.clone();
        let user = user.clone();
        let pass = pass.clone();

        let mpb = mpb.clone();
        let pb = mpb.add(ProgressBar::new(1));
        let style = ProgressStyle::default_bar()
            .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {msg:.yellow.bold}")
            .progress_chars("##-");
        pb.set_style(style);
        let slice_num_width = slice.to_string().len();
        pb.set_prefix(&format!(
            "[{:0width$}/{}]",
            slice_id + 1,
            slice,
            width = slice_num_width
        ));
        pb.set_message("Starting...");

        pool.execute(move || {
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

            let (docs, mut scroll_id, total) = parse_response(res).expect("error parsing response");

            let style = ProgressStyle::default_bar()
                .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.yellow.bold}")
                .progress_chars("##-");
            pb.set_message("Running...");
            pb.set_style(style);
            pb.set_length(total);
            pb.set_draw_delta(max(1, min(10000, total / 100)));
            pb.inc(docs.len() as u64);

            let mut finished = docs.is_empty();
            tx.send(Box::new(docs)).expect("error sending result to channel");

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

                let (docs, new_scroll_id, total) = parse_response(res).expect("error parsing response");

                scroll_id = new_scroll_id;
                pb.set_length(total);
                pb.inc(docs.len() as u64);
                finished = docs.is_empty();
                tx.send(Box::new(docs)).expect("error sending result to channel");
            }

            let style = ProgressStyle::default_bar()
                .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.green.bold}")
                .progress_chars("##-");
            pb.set_style(style);
            pb.finish_with_message("Finished.")
        });
    }

    thread::spawn(move || {
        pool.join();
        drop(tx);
    });

    let output = output;
    let output_thread = thread::spawn(move || {
        let mut output = BufWriter::new(File::create(output).unwrap());
        for docs in rx.iter() {
            for doc in docs.iter() {
                writeln!(&mut output, "{}", doc).unwrap();
            }
        }
    });

    mpb.join()?;
    output_thread.join().unwrap();
    Ok(())
}