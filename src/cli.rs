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
    Pull(PullOpt),
    /// Generate shell completion file
    #[structopt(name = "completion")]
    #[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
    Completion(CompletionOpt),
    /// Check for updates
    #[structopt(name = "update")]
    #[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
    Update,
}

#[derive(StructOpt, Debug)]
pub struct CompletionOpt {
    /// Target shell name
    pub shell: clap::Shell,

    /// Completion file directory
    #[structopt(short = "o", long = "output", default_value = "")]
    pub output: PathBuf,
}

#[derive(StructOpt, Debug)]
pub struct PullOpt {
    /// ElasticSearch host url
    #[structopt(short = "h", long = "host", default_value = "http://localhost:9200")]
    pub host: String,

    /// Username for http basic authorization
    #[structopt(short = "u", long = "user")]
    pub user: Option<String>,

    /// Target index name(or alias)
    #[structopt(short = "i", long = "index")]
    pub index: String,

    /// File path for query body
    #[structopt(short = "q", long = "query")]
    pub query: Option<PathBuf>,

    /// Scroll slice count
    #[structopt(short = "s", long = "slice", default_value = "1")]
    pub slice: u32,

    /// Scroll batch size (if null size in query body will be used)
    #[structopt(short = "b", long = "batch")]
    pub batch: Option<u32>,

    /// Max entries to download
    #[structopt(short = "l", long = "limit")]
    pub limit: Option<u64>,

    /// File path for output
    #[structopt(short = "o", long = "output", default_value = "/dev/stdout")]
    pub output: PathBuf,

    /// Scroll session ttl
    #[structopt(long = "ttl", default_value = "1m")]
    pub ttl: String,
}
