use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "velox", version, about = "Velox CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Build a .vx/.vue Single File Component into Rust.
    Build {
        /// Path to .vx/.vue file
        input: PathBuf,
        /// Output directory (default: target/velox-gen)
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// What to emit: stub constants or a render() function
        #[arg(long, value_enum, default_value_t = velox_cli::EmitMode::Stub)]
        emit: velox_cli::EmitMode,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build {
            input,
            out_dir,
            emit,
        } => velox_cli::build_cmd(&input, out_dir.as_deref(), emit)?,
    }
    Ok(())
}
