use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EmitMode {
    Stub,
    Render,
}

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
        #[arg(long, value_enum, default_value_t = EmitMode::Stub)]
        emit: EmitMode,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Build {
            input,
            out_dir,
            emit,
        } => build_cmd(&input, out_dir.as_deref(), emit)?,
    }
    Ok(())
}

fn build_cmd(input: &Path, out_dir: Option<&Path>, emit: EmitMode) -> Result<()> {
    let src =
        fs::read_to_string(input).with_context(|| format!("failed to read {}", input.display()))?;

    let sfc = velox_sfc::parse_sfc(&src).map_err(|e| anyhow::anyhow!(e))?;

    let name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component");

    let mut code = String::new();

    match emit {
        EmitMode::Stub => {
            code.push_str(&velox_sfc::to_stub_rs(&sfc, name));
        }
        EmitMode::Render => {
            let tpl_src = sfc
                .template
                .as_ref()
                .map(|t| t.content.as_str())
                .unwrap_or("");
            let render_fn =
                velox_sfc::compile_template_to_rs(tpl_src, name).map_err(|e| anyhow::anyhow!(e))?;
            // Emit both stub constants and render() in one file
            code.push_str(&velox_sfc::to_stub_rs(&sfc, name));
            code.push_str("\n");
            code.push_str(&render_fn);
            code.push_str("\n");
        }
    }

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
