use std::path::PathBuf;
use structopt::StructOpt;

#[derive(StructOpt, Debug)]
#[structopt(raw(setting = "structopt::clap::AppSettings::ColoredHelp"))]
pub struct Opt {
    #[structopt(short = "h", long = "host", default_value = "http://localhost:9200")]
    pub host: String,

    #[structopt(short = "u", long = "user")]
    pub user: Option<String>,

    #[structopt(short = "i", long = "index")]
    pub index: String,

    #[structopt(short = "q", long = "query")]
    pub query: PathBuf,

    #[structopt(short = "s", long = "slice", default_value = "1")]
    pub slice: u32,

    #[structopt(short = "b", long = "batch")]
    pub batch: Option<u32>,

    #[structopt(short = "o", long = "output")]
    pub output: Option<PathBuf>,

    #[structopt(long = "scroll-ttl", default_value = "1m")]
    pub scroll_ttl: String,
}
