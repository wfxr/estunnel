use std::path::PathBuf;
use structopt::clap;

pub use structopt::clap::Shell;
pub use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
pub enum Opt {
    /// Pull data from ElasticSearch
    #[structopt(name = "pull")]
    #[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
    Pull {
        /// ElasticSearch host url
        #[structopt(short = "h", long = "host", default_value = "http://localhost:9200")]
        host: String,

        /// Username for http basic authorization
        #[structopt(short = "u", long = "user")]
        user: Option<String>,

        /// Target index name(or alias)
        #[structopt(short = "i", long = "index")]
        index: String,

        /// File path for query body
        #[structopt(short = "q", long = "query")]
        query: PathBuf,

        /// Scroll slice count
        #[structopt(short = "s", long = "slice", default_value = "1")]
        slice: u32,

        /// Scroll batch size. Size in query will be used if null.
        #[structopt(short = "b", long = "batch")]
        batch: Option<u32>,

        /// File path for output
        #[structopt(short = "o", long = "output", default_value = "/dev/stdout")]
        output: PathBuf,

        /// Scroll session ttl
        #[structopt(long = "ttl", default_value = "1m")]
        ttl: String,
    },
    /// Generate shell completion file
    #[structopt(name = "completion")]
    #[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
    Completion {
        #[structopt(help = "Target shell name")]
        shell: clap::Shell,

        /// Completion file directory
        #[structopt(short = "o", long = "output", default_value = "")]
        output: PathBuf,
    },
}
