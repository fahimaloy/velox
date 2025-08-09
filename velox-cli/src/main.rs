use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "velox", version, about = "Velox CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a .vue-like Single File Component into a Rust stub.
    Build {
        /// Path to .vue file
        input: PathBuf,
        /// Output directory (default: target/velox-gen)
        #[arg(long)]
        out_dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build { input, out_dir } => build_cmd(&input, out_dir.as_deref())?,
    }
    Ok(())
}

fn build_cmd(input: &Path, out_dir: Option<&Path>) -> Result<()> {
    let src =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;

    // parse_sfc returns Result<_, String>; map it into anyhow::Error
    let sfc = velox_sfc::parse_sfc(&src).map_err(|e| anyhow::anyhow!(e))?;

    let name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component");

    let code = velox_sfc::to_stub_rs(&sfc, name);

    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("target/velox-gen"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let out_path = out_dir.join(format!("{}.rs", name));
    fs::write(&out_path, code)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    println!("Generated: {}", out_path.display());
    Ok(())
}
