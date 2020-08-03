mod cli;
mod cmd;
mod common;
mod elastic;

use cli::*;
use common::*;

fn main() -> Result<()> {
    match Opt::from_args() {
        Opt::Completion(completion) => cmd::completion::completion(completion),
        Opt::Pull(pull) => cmd::pull::pull(pull),
        Opt::Update => cmd::update::update(),
    }
}
