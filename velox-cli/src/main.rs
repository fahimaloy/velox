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
    /// Build the current project, or compile a specific .vx/.vue file into Rust.
    Build {
        /// Optional path to .vx/.vue file. If omitted, builds the current project.
        input: Option<PathBuf>,
        /// Output directory (used only when building a single .vx/.vue file)
        #[arg(long)]
        out_dir: Option<PathBuf>,
        /// What to emit when building a single .vx/.vue file
        #[arg(long, value_enum, default_value_t = velox_cli::EmitMode::Stub)]
        emit: velox_cli::EmitMode,
        /// Build in release mode when building the current project
        #[arg(long)]
        release: bool,
    },
    /// Initialize a new Velox app in the current directory
    Init { name: String },
    /// Run the current project (cargo run)
    Run,
    /// Dev server: restart current project on file changes (polling)
    Dev {
        #[arg(long)]
        watch: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build {
            input,
            out_dir,
            emit,
            release,
        } => {
            if let Some(input) = input {
                velox_cli::build_cmd(&input, out_dir.as_deref(), emit)?;
            } else {
                velox_cli::build_current(release)?;
            }
        }
        Commands::Init { name } => {
            let path = velox_cli::init_project(&name)?;
            println!("Initialized app at {}", path.display());
        }
        Commands::Run => velox_cli::run_current()?,
        Commands::Dev { watch } => {
            let dir = watch.unwrap_or_else(|| PathBuf::from("src"));
            velox_cli::dev_current(&dir)?;
        }
    }
    Ok(())
}
