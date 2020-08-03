use crate::cli::{CompletionOpt, Opt};
use crate::common::Result;
use structopt::StructOpt;

pub fn completion(opt: CompletionOpt) -> Result<()> {
    let CompletionOpt { shell, output } = opt;
    Opt::clap().gen_completions(env!("CARGO_PKG_NAME"), shell, output);
    Ok(())
}
