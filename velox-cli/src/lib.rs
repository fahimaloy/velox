//! Velox CLI - Build tooling and dev server

pub mod commands;

use anyhow::Result;
use clap::ValueEnum;
use std::path::{Path, PathBuf};

/// Normalize and validate a Cargo package name.
///
/// Dots and whitespace are converted to hyphens to match user expectations.
pub fn validate_and_normalize_package_name(name: &str) -> Result<String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        anyhow::bail!("package name cannot be empty");
    }

    let normalized = trimmed
        .replace('.', "-")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-");

    let mut chars = normalized.chars();
    let Some(first) = chars.next() else {
        anyhow::bail!("package name cannot be empty");
    };

    if !first.is_ascii_alphabetic() && first != '_' {
        anyhow::bail!("invalid package name '{trimmed}': first character must be a letter or '_'");
    }

    if let Some((idx, ch)) = normalized
        .chars()
        .enumerate()
        .find(|(_, ch)| !ch.is_ascii_alphanumeric() && *ch != '_' && *ch != '-')
    {
        anyhow::bail!(
            "invalid package name '{trimmed}': character '{ch}' at position {idx} is not allowed"
        );
    }

    Ok(normalized)
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum EmitMode {
    Stub,
    Render,
}

/// Build a .vx/.vue file into a Rust module written to `out_dir`.
///
/// In `Render` mode, this recursively compiles the input .vx file and all
/// its imported component dependencies. Each component is written to a
/// separate `.rs` file in `out_dir`.
///
/// In `Stub` mode, only the single input file is compiled (no recursive imports).
///
/// After compilation, `cargo:rerun-if-changed` directives are emitted for
/// all .vx files that were read, so Cargo knows when to re-run the build script.
pub fn build_cmd(input: &Path, out_dir: Option<&Path>, emit: EmitMode) -> Result<()> {
    use anyhow::Context;
    use std::fs;

    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("target/velox-gen"));
    fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    match emit {
        EmitMode::Stub => {
            // Stub mode: single-file compilation, no recursive imports
            let src = fs::read_to_string(input)
                .with_context(|| format!("failed to read {}", input.display()))?;
            let sfc = velox_sfc::parse_sfc(&src).map_err(|e| anyhow::anyhow!(e))?;
            let name = input
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("component");

            let code = velox_sfc::to_stub_rs(&sfc, name);
            let out_path = out_dir.join(format!("{}.rs", name));
            fs::write(&out_path, code)
                .with_context(|| format!("failed to write {}", out_path.display()))?;
            println!("Generated: {}", out_path.display());
        }
        EmitMode::Render => {
            // Render mode: recursively compile the input and all imported components
            let result = commands::build::build_vx(input, Some(&out_dir))
                .with_context(|| "failed to compile component tree")?;

            // Emit cargo:rerun-if-changed for every .vx file that was read
            for vx_file in &result.vx_files {
                println!("cargo:rerun-if-changed={}", vx_file.display());
            }
        }
    }

    Ok(())
}

/// Re-export commands
pub use commands::*;

pub const VELOX_GIT_REV_FALLBACK: &str = "unknown";
pub fn velox_git_rev() -> &'static str {
    option_env!("VELOX_GIT_REV").unwrap_or(VELOX_GIT_REV_FALLBACK)
}
