use crate::common::Result;
use regex::Regex;

pub fn update() -> Result<()> {
    let target = self_update::get_target();
    let repo = env!("CARGO_PKG_REPOSITORY");
    let repo_caps = Regex::new(r#"github.com/(?P<owner>\w+)/(?P<name>\w+)$"#)
        .unwrap()
        .captures(repo)
        .unwrap();
    let repo_owner = repo_caps.name("owner").unwrap().as_str();
    let repo_name = repo_caps.name("name").unwrap().as_str();

    let status = self_update::backends::github::Update::configure()
        .repo_owner(repo_owner)
        .repo_name(repo_name)
        .target(&target)
        .bin_name(env!("CARGO_PKG_NAME"))
        .show_download_progress(true)
        .current_version(self_update::cargo_crate_version!())
        .build()?
        .update()?;

    if status.updated() {
        println!("Upgrade to version {} successfully!", status.version())
    } else {
        println!("The current version is up to date.")
    }
    Ok(())
}
