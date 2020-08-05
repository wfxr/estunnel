use crate::cli::{CompletionOpt, Opt};
use crate::common::Result;
use structopt::StructOpt;

pub fn completion(opt: CompletionOpt) -> Result<()> {
    let CompletionOpt { shell } = opt;
    Opt::clap().gen_completions_to(env!("CARGO_PKG_NAME"), shell, &mut std::io::stdout());
    Ok(())
}
