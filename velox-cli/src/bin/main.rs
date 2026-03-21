use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "velox")]
#[command(version = "0.1.0")]
#[command(author = "fahimaloy")]
#[command(about = "🚀 Velox - A modern UI framework for Rust", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new Velox project
    #[command(about = "Create a new Velox project with scaffolding")]
    Init {
        /// Project name
        name: String,
    },

    /// Build a .vx component to Rust
    #[command(about = "Compile a .vx Single File Component")]
    Build {
        /// Path to .vx file
        input: PathBuf,
        /// Output directory
        #[arg(long, short = 'o')]
        out_dir: Option<PathBuf>,
    },

    /// Run a Velox project
    #[command(about = "Build and run a project")]
    Run {
        /// Optional release build
        #[arg(long)]
        release: bool,
    },

    /// Development server with hot reload
    #[command(about = "Start dev server with file watching")]
    Dev {
        /// Watch directory
        #[arg(long, short = 'w')]
        watch: Option<PathBuf>,
    },

    /// Lint .vx files for syntax errors
    #[command(about = "Check .vx files for issues")]
    Lint {
        /// File or directory to lint
        target: Option<PathBuf>,
    },

    /// Show version and system info
    #[command(about = "Display version information")]
    Version,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name } => {
            let path = velox_cli::commands::init_project(&name)?;
            println!("✅ Created Velox project at: {}", path.display());
            println!("\n📖 Next steps:");
            println!("   cd {}", name);
            println!("   velox dev");
        }

        Commands::Build { input, out_dir } => {
            velox_cli::build_cmd(&input, out_dir.as_deref(), velox_cli::EmitMode::Render)?;
        }

        Commands::Run { release } => {
            if release {
                println!("🔨 Building in release mode...");
                velox_cli::commands::build_app(".", true)?;
            }
            println!("▶️  Running project...");
            velox_cli::commands::run_current()?;
        }

        Commands::Dev { watch } => {
            let dir = watch.unwrap_or_else(|| PathBuf::from("src"));
            println!("👀 Watching {} for changes...", dir.display());
            println!("Press 'r' to reload, 'q' to quit\n");
            velox_cli::commands::dev_current(&dir)?;
        }

        Commands::Lint { target } => {
            let dir = target.unwrap_or_else(|| PathBuf::from("src"));
            println!("🔍 Linting .vx files in {}...", dir.display());
            lint_directory(&dir)?;
        }

        Commands::Version => {
            println!("velox {}", env!("CARGO_PKG_VERSION"));
            println!("Edition: 2021");
            #[cfg(target_os = "linux")]
            println!("Platform: Linux");
            #[cfg(target_os = "macos")]
            println!("Platform: macOS");
            #[cfg(target_os = "windows")]
            println!("Platform: Windows");
        }
    }

    Ok(())
}

/// Simple directory linting
fn lint_directory(dir: &std::path::Path) -> Result<()> {
    fn walk(dir: &std::path::Path, file_count: &mut usize, error_count: &mut usize) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                walk(&path, file_count, error_count)?;
                continue;
            }

            if path.extension().and_then(|s| s.to_str()) == Some("vx") {
                *file_count += 1;
                match std::fs::read_to_string(&path) {
                    Ok(content) => match velox_sfc::parse_sfc(&content) {
                        Ok(_) => println!("✅ {}", path.display()),
                        Err(e) => {
                            *error_count += 1;
                            println!("❌ {} - {}", path.display(), e);
                        }
                    },
                    Err(e) => {
                        *error_count += 1;
                        println!("❌ {} - Read error: {}", path.display(), e);
                    }
                }
            }
        }
        Ok(())
    }

    let mut file_count = 0usize;
    let mut error_count = 0usize;
    walk(dir, &mut file_count, &mut error_count)?;

    if file_count == 0 {
        println!("⚠️  No .vx files found in {}", dir.display());
    } else {
        println!("\n📊 Lint results: {} files, {} errors", file_count, error_count);
    }

    if error_count > 0 {
        anyhow::bail!("Lint failed with {} errors", error_count);
    }

    Ok(())
}
