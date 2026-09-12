use anyhow::Result;
use std::process::Command;

/// Run an app package via cargo run
pub fn run_app(pkg: &str) -> Result<()> {
    let status = Command::new("cargo").args(["run", "-p", pkg]).status()?;
    if !status.success() {
        anyhow::bail!("app run failed")
    }
    Ok(())
}

/// Run the current project
pub fn run_current(release: bool) -> Result<()> {
    let mut args = vec!["run"];
    if release {
        args.push("--release");
    }
    let status = Command::new("cargo").args(&args).status()?;
    if !status.success() {
        anyhow::bail!("run failed")
    }
    Ok(())
}
