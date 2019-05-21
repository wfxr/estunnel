mod cli;
mod cmd;
mod common;
mod elastic;

use cli::*;
use common::*;

fn main() -> Result<()> {
    match Opt::from_args() {
        Opt::Completion { shell, output } => cmd::completion(shell, output),
        Opt::Pull {
            host,
            user,
            index,
            query,
            slice,
            batch,
            output,
            ttl,
        } => cmd::pull(host, user, index, query, slice, batch, output, ttl),
    }
}
