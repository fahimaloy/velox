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
    /// Initialize a new Velox app under examples/<name>
    Init { name: String },
    /// Run an app package (cargo run -p <pkg>)
    Run { package: String },
    /// Build an app package (cargo build -p <pkg>)
    BuildApp { package: String, #[arg(long)] release: bool },
    /// Dev server: restart app on file changes (polling)
    Dev { package: String, #[arg(long)] watch: Option<PathBuf> },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build {
            input,
            out_dir,
            emit,
        } => velox_cli::build_cmd(&input, out_dir.as_deref(), emit)?,
        Commands::Init { name } => {
            let path = velox_cli::init_app(&name)?;
            println!("Initialized app at {}", path.display());
        }
        Commands::Run { package } => velox_cli::run_app(&package)?,
        Commands::BuildApp { package, release } => velox_cli::build_app(&package, release)?,
        Commands::Dev { package, watch } => {
            let dir = watch.unwrap_or_else(|| PathBuf::from(format!("examples/{}", package)));
            velox_cli::dev_app(&package, &dir)?;
        }
    }
    Ok(())
}
