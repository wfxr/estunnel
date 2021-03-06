use crate::cli::PullOpt;
use crate::common::Result;
use crate::elastic::*;
use crossbeam::{crossbeam_channel, Receiver, Sender};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use serde::export::fmt::Display;
use serde_json::{json, Value};
use std::cmp::{max, min};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub fn pull(opt: PullOpt) -> Result<()> {
    let PullOpt {
        host,
        user,
        index,
        query,
        slice,
        batch,
        limit,
        output,
        ttl,
    } = opt;
    let (user, pass) = userpass(user);

    let query: serde_json::Value = match query {
        Some(query) => serde_json::from_reader(BufReader::new(File::open(query)?))?,
        None => json!({ "query": { "match_all": {} } }),
    };
    let batch = match batch {
        Some(batch) => batch,
        None => query["size"].as_u64().unwrap_or(1000),
    };
    let batch = match limit {
        Some(limit) => min(batch, max(limit / slice, 1)),
        None => batch,
    };

    let (res_tx, res_rx) = crossbeam_channel::bounded(slice as usize);
    let (err_tx, err_rx) = crossbeam_channel::unbounded();
    let task_finished = Arc::new(AtomicBool::new(false));

    let (mpb, task_pb) = create_pb(limit);
    let pool = threadpool::ThreadPool::new(slice as usize);

    for slice_id in 0..slice {
        pool.execute({
            let res_tx = res_tx.clone();
            let err_tx = err_tx.clone();
            let mut query = query.clone();
            let host = host.clone();
            let index = index.clone();
            let scroll_ttl = ttl.clone();
            let user = user.clone();
            let pass = pass.clone();
            let task_finished = task_finished.clone();
            let job_id = slice_id + 1;
            let pb = create_pb_child(&slice, &mpb, &job_id);
            move || {
                let client = reqwest::blocking::Client::new();
                query = inject_query(slice, slice_id, query);

                let url = format!("{}/{}/_search", &host, &index);
                let params = vec![("scroll", scroll_ttl.to_string()), ("size", batch.to_string())].into();
                let res = request_elastic(&client, &url, &query, &user, &pass, &params);

                let res = match res {
                    Ok(res) => res,
                    Err(e) => {
                        err_tx.send(format!("Fetch error[{}]: {}", job_id, e))
                            .expect("error sending to channel");
                        pb.finish_at_current_pos();
                        return;
                    }
                };

                let (mut finished, mut scroll_id) = match parse_response(res) {
                    Ok((docs, scroll_id, total)) => {
                        let style = ProgressStyle::default_bar()
                            .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.yellow.bold}")
                            .progress_chars("##-");
                        pb.set_message("Running...");
                        pb.set_style(style);

                        let finished = docs.is_empty() || task_finished.load(Ordering::Relaxed);
                        if !finished {
                            send_docs(&res_tx, &pb, docs, total);
                        }
                        (finished, scroll_id)
                    }
                    Err(e) => {
                        err_tx.send(format!("Parse error[{}]: {}", job_id, e))
                            .expect("error sending to channel");
                        pb.finish_at_current_pos();
                        return;
                    }
                };

                while !finished {
                    let url = format!("{}/_search/scroll", &host);
                    let query = json!({ "scroll": scroll_ttl, "scroll_id": scroll_id, });
                    let res = request_elastic(&client, &url, &query, &user, &pass, &None);

                    let res = match res {
                        Ok(res) => res,
                        Err(e) => {
                            err_tx.send(format!("Error[{}]: {}", job_id, e))
                                .expect("error sending to channel");
                            pb.finish_at_current_pos();
                            return;
                        }
                    };
                    match parse_response(res) {
                        Ok((docs, new_scroll_id, total)) => {
                            finished = docs.is_empty() || task_finished.load(Ordering::Relaxed);
                            scroll_id = new_scroll_id;
                            if !finished {
                                send_docs(&res_tx, &pb, docs, total);
                            }
                        }
                        Err(e) => {
                            err_tx.send(format!("Error[{}]: {}", job_id, e))
                                .expect("error sending to channel");
                            pb.finish_and_clear();
                            return;
                        }
                    }
                }
                finish_pb(pb);
            }
        });
    }

    let output_thread = thread::spawn({
        let err_tx = err_tx.clone();
        move || match sink(limit, output, &res_rx, task_finished.clone(), &task_pb) {
            Err(e) => {
                task_finished.store(true, Ordering::Relaxed);
                err_tx
                    .send(format!("Write error: {}", e))
                    .expect("error sending error to channel");
                if let Some(task_pb) = task_pb {
                    task_pb.finish_at_current_pos();
                }
            }
            Ok(curr) => {
                if let Some(task_pb) = task_pb {
                    let style = ProgressStyle::default_bar().template(
                        "{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise}",
                    );
                    task_pb.set_style(style);
                    if let Some(limit) = limit {
                        if curr >= limit {
                            task_pb.finish_with_message("Finished.")
                        } else {
                            task_pb.finish_at_current_pos();
                        }
                    }
                }
            }
        }
    });

    thread::spawn(move || {
        pool.join();
        drop(res_tx);
    });

    mpb.join().expect("error joining progress threads");

    output_thread.join().unwrap();
    drop(err_tx);

    // print error if any
    for err in err_rx {
        eprintln!("{}", err)
    }

    Ok(())
}

fn sink<T: Display + ?Sized>(
    limit: Option<u64>,
    output: PathBuf,
    res_rx: &Receiver<Vec<Box<T>>>,
    task_finished: Arc<AtomicBool>,
    task_pb: &Option<ProgressBar>,
) -> Result<u64> {
    let mut output = BufWriter::new(File::create(output)?);
    let mut curr = 0u64;
    for docs in res_rx.iter() {
        for doc in docs.iter() {
            if let Some(limit) = limit {
                if curr >= limit {
                    task_finished.store(true, Ordering::Relaxed);
                    return Ok(curr);
                }
            }
            match writeln!(&mut output, "{}", doc) {
                Ok(_) => {
                    curr += 1;
                    if let Some(pb) = &task_pb {
                        pb.inc(1)
                    }
                }
                Err(_e) => {
                    task_finished.store(true, Ordering::Relaxed);
                    // https://github.com/rust-lang/rust/issues/46016
                    // should just silently exit in this case to avoid broken pipe error
                    return Ok(curr);
                    // return Err(Box::new(e));
                }
            };
        }
    }
    Ok(curr)
}

fn send_docs<T>(tx: &Sender<Vec<T>>, pb: &ProgressBar, docs: Vec<T>, total: u64) {
    let len = docs.len() as u64;
    tx.send(docs).expect("error sending to channel");
    pb.set_length(total);
    pb.inc(len);
}

fn inject_query(slice: u64, slice_id: u64, mut query: Value) -> Value {
    if slice > 1 {
        let obj = query.as_object_mut().unwrap();
        obj.insert(
            "slice".into(),
            json!({
                "id": slice_id,
                "max": slice
            }),
        );
        return json!(obj);
    }
    query
}

fn create_pb_child(slice: &u64, mpb: &Arc<MultiProgress>, job_id: &u64) -> ProgressBar {
    let mpb = mpb.clone();
    let pb = mpb.add(ProgressBar::new(1));
    let style = ProgressStyle::default_bar()
        .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {msg:.yellow.bold}")
        .progress_chars("##-");
    pb.set_style(style);
    let slice_num_width = slice.to_string().len();
    pb.set_prefix(&format!("[{:0width$}/{}]", job_id, slice, width = slice_num_width));
    pb.set_message("Starting...");
    pb.set_draw_delta(1_000_000);
    pb.enable_steady_tick(100);
    pb
}

fn create_pb(limit: Option<u64>) -> (Arc<MultiProgress>, Option<ProgressBar>) {
    let mpb = Arc::new(MultiProgress::new());
    let task_pb = limit.map(|limit| {
        let pb = mpb.add(ProgressBar::new(limit));
        let style = ProgressStyle::default_bar()
            .template("{prefix:.blue.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise}");
        pb.set_prefix("Task:");
        pb.set_style(style);
        pb.set_length(limit);
        pb.set_draw_delta(1_000_000);
        pb.enable_steady_tick(100);
        pb
    });
    (mpb, task_pb)
}

fn finish_pb(pb: ProgressBar) {
    let style = ProgressStyle::default_bar()
        .template(
            "{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.green.bold}",
        )
        .progress_chars("##-");
    pb.set_length(pb.position()); // adjust length
    pb.set_style(style);
    pb.finish_with_message("Finished.");
}

fn userpass(auth: Option<String>) -> (String, Option<String>) {
    match auth {
        Some(auth) => {
            let id = auth.find(':');
            if let Some(id) = id {
                let user = &auth[0..id];
                let pass = &auth[id + 1..];
                (user.to_string(), Some(pass.to_string()))
            } else {
                let prompt = format!("Enter host password for user {}: ", auth);
                let pass = rpassword::read_password_from_tty(Some(&prompt)).unwrap();
                (auth, Some(pass))
            }
        }
        None => ("estunnel".to_string(), None),
    }
}
