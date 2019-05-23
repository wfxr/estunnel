use crate::cli::{CompletionOpt, Opt, PullOpt, StructOpt};
use crate::common::Result;
use crate::elastic::*;
use crossbeam::crossbeam_channel;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use regex;
use regex::Regex;
use self_update;
use serde_json::json;
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

pub fn completion(opt: CompletionOpt) -> Result<()> {
    let CompletionOpt { shell, output } = opt;
    Opt::clap().gen_completions(env!("CARGO_PKG_NAME"), shell, output);
    Ok(())
}

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
    let pass = match &user {
        Some(user) => {
            let prompt = format!("Enter host password for user {}: ", user.clone());
            Some(rpassword::read_password_from_tty(Some(&prompt)).unwrap())
        }
        None => None,
    };
    let user = user.unwrap_or_else(|| "estunnel".to_owned());

    let query = BufReader::new(File::open(query)?);
    let query: serde_json::Value = serde_json::from_reader(query)?;

    let (res_tx, res_rx) = crossbeam_channel::bounded(slice as usize);
    let (err_tx, err_rx) = crossbeam_channel::unbounded();
    let task_finished = Arc::new(AtomicBool::new(false));

    let mpb = Arc::new(MultiProgress::new());
    let pool = threadpool::ThreadPool::new(slice as usize);

    let pb = limit.map(|limit| {
        let pb = mpb.add(ProgressBar::new(limit));
        let style = ProgressStyle::default_bar()
            .template("{prefix:.blue.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.yellow.bold}");
        pb.set_prefix("Task:");
        pb.set_message("Running...");
        pb.set_style(style);
        pb.set_length(limit);
        pb.set_draw_delta(1000);
        pb
    });
    for slice_id in 0..slice {
        let res_tx = res_tx.clone();
        let err_tx = err_tx.clone();
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

        let task_finished = task_finished.clone();
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

            let res = request_elastic(
                &client,
                &format!("{}/{}/_search", &host, &index),
                &query,
                &user,
                &pass,
                Some(params),
            );

            let res = match res {
                Ok(res) => res,
                Err(e) => {
                    err_tx.send(format!("Fetch error[{}]: {}", slice_id, e))
                        .expect("error sending to channel");
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
                    pb.set_length(total);
                    pb.inc(docs.len() as u64);

                    let finished = docs.is_empty() || task_finished.load(Ordering::Relaxed);
                    if !finished {
                        res_tx.send(Box::new(docs)).expect("error sending to channel");
                    }
                    (finished, scroll_id)
                }
                Err(e) => {
                    err_tx.send(format!("Parse error[{}]: {}", slice_id, e))
                        .expect("error sending to channel");
                    return;
                }
            };

            while !finished {
                let res = request_elastic(
                    &client,
                    &format!("{}/_search/scroll", &host),
                    &json!({
                        "scroll": scroll_ttl,
                        "scroll_id": scroll_id,
                    }),
                    &user,
                    &pass,
                    None,
                );

                let res = match res {
                    Ok(res) => res,
                    Err(e) => {
                        err_tx.send(format!("Error[{}]: {}", slice_id, e))
                            .expect("error sending to channel");
                        return;
                    }
                };
                match parse_response(res) {
                    Ok((docs, new_scroll_id, total)) => {
                        scroll_id = new_scroll_id;
                        pb.set_length(total);
                        pb.inc(docs.len() as u64);
                        finished = docs.is_empty() || task_finished.load(Ordering::Relaxed);
                        if !finished {
                            res_tx.send(Box::new(docs)).expect("error sending to channel");
                        }
                    }
                    Err(e) => {
                        err_tx.send(format!("Error[{}]: {}", slice_id, e))
                            .expect("error sending to channel");
                        return;
                    }
                }
            }

            let style = ProgressStyle::default_bar()
                .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.green.bold}")
                .progress_chars("##-");
            pb.set_style(style);
            pb.set_message("Finished.");
        });
    }

    {
        thread::spawn(move || {
            pool.join();
            drop(res_tx);
            drop(err_tx);
        });
    }

    let output_thread = thread::spawn(move || {
        let mut output = BufWriter::new(File::create(output)?);
        let mut curr = 0u64;
        for docs in res_rx.iter() {
            for doc in docs.iter() {
                match writeln!(&mut output, "{}", doc) {
                    Ok(_) => {
                        curr += 1;
                        if let Some(pb) = &pb {
                            pb.inc(1)
                        }
                    }
                    Err(e) => return Err(Box::new(e)),
                };
                if let Some(limit) = limit {
                    if curr >= limit {
                        task_finished.store(true, Ordering::Relaxed);
                        if let Some(pb) = &pb {
                            let style = ProgressStyle::default_bar()
                                .template("{prefix:.bold} {elapsed_precise} {bar:50} {percent:>3}% {pos}/{len} ETA {eta_precise} {msg:.green.bold}");
                            pb.set_style(style);
                            pb.finish_with_message("Finished.");
                        }
                        break;
                    }
                }
            }
        }
        Ok(())
    });

    mpb.join()?;
    output_thread.join().unwrap()?;

    // print error if any
    for err in err_rx {
        eprintln!("{}", err)
    }

    Ok(())
}

pub fn update() -> Result<()> {
    let target = self_update::get_target()?;
    let repo = env!("CARGO_PKG_REPOSITORY");
    let repo_caps = Regex::new(r#"github.com/(?P<owner>\w+)/(?P<name>\w+)$"#)
        .unwrap()
        .captures(repo)
        .unwrap();
    let repo_owner = repo_caps.name("owner").unwrap().as_str();
    let repo_name = repo_caps.name("name").unwrap().as_str();

    let status = self_update::backends::github::Update::configure()?
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .target(&target)
        .bin_name(env!("CARGO_PKG_NAME"))
        .show_download_progress(true)
        .current_version(self_update::cargo_crate_version!())
        .build()?
        .update()?;

    match status.updated() {
        true => println!("Upgrade to version {} successfully!", status.version()),
        false => println!("The current version is up to date."),
    }
    Ok(())
}
