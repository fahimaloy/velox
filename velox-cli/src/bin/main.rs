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
        name: String,
        #[arg(long, short = 't', default_value = "default")]
        template: String,
        #[arg(long)]
        local: Option<std::path::PathBuf>,
    },

    /// Build the current project, or compile a .vx component to Rust
    #[command(about = "Build project (default) or compile a .vx component")]
    Build {
        /// Optional path to .vx file. If omitted, builds current project.
        input: Option<PathBuf>,
        /// Output directory when compiling a .vx file
        #[arg(long, short = 'o')]
        out_dir: Option<PathBuf>,
        /// Release mode when building current project
        #[arg(long)]
        release: bool,
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
        /// Watch directory (project root; builds with cargo run)
        #[arg(long, short = 'w')]
        watch: Option<PathBuf>,
        /// Release mode build
        #[arg(long)]
        release: bool,
    },

    /// Lint .vx files for syntax errors
    #[command(about = "Check .vx files for issues")]
    Lint {
        /// File or directory to lint
        target: Option<PathBuf>,
        /// Auto-fix fixable issues (trailing whitespace, missing final newline)
        #[arg(long)]
        fix: bool,
    },

    /// Add a component or other scaffold
    #[command(about = "Add a component or other scaffold to the project")]
    Add {
        #[command(subcommand)]
        what: AddCommand,
    },

    /// Show version and system info
    #[command(about = "Display version information")]
    Version,
}

#[derive(Subcommand)]
enum AddCommand {
    /// Add a new component (.vx file)
    #[command(about = "Generate a new component .vx file")]
    Component {
        /// Component name (e.g. "Counter" or "side-bar")
        name: String,
    },
}

fn main() -> Result<()> {
    env_logger::init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Init { name, template, local } => {
            let path = velox_cli::commands::init_project_with_template_local(&name, &template, local.as_deref())?;
            println!("✅ Created Velox project at: {}", path.display());
            println!("\n📖 Next steps:");
            println!("   cd {}", path.display());
            println!("   velox dev");
        }

        Commands::Build {
            input,
            out_dir,
            release,
        } => {
            if let Some(input) = input {
                velox_cli::build_cmd(&input, out_dir.as_deref(), velox_cli::EmitMode::Render)?;
                println!("ℹ️  The generated .rs file is source code, not an executable.");
            } else {
                velox_cli::commands::build_current(release)?;
            }
        }

        Commands::Run { release } => {
            if release {
                println!("🔨 Building in release mode...");
                velox_cli::commands::build_current(true)?;
            }
            println!("▶️  Running project...");
            velox_cli::commands::run_current(release)?;
        }

        Commands::Dev { watch, release } => {
            let dir = watch.unwrap_or_else(|| PathBuf::from("."));
            velox_cli::commands::dev_current(&dir, release)?;
        }

        Commands::Lint { target, fix } => {
            let target = target.unwrap_or_else(|| PathBuf::from("src"));
            if target.is_file() {
                // A single .vx file was given.
                if fix {
                    println!("🔧 Lint+fix {}...", target.display());
                    velox_cli::commands::fix_file_single(&target)?;
                } else {
                    println!("🔍 Linting {}...", target.display());
                    velox_cli::commands::lint_file(&target)?;
                }
            } else {
                // A directory (or the default "src") — walk recursively.
                if fix {
                    println!("🔧 Lint+fix .vx files in {}...", target.display());
                    velox_cli::commands::lint_directory_fix(&target, true)?;
                } else {
                    println!("🔍 Linting .vx files in {}...", target.display());
                    velox_cli::commands::lint_directory_fix(&target, false)?;
                }
            }
        }

        Commands::Add { what } => match what {
            AddCommand::Component { name } => {
                velox_cli::commands::add_component(&name)?;
            }
        },

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
