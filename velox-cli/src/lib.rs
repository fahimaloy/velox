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
        anyhow::bail!(
            "invalid package name '{trimmed}': first character must be a letter or '_'"
        );
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
pub fn build_cmd(input: &Path, out_dir: Option<&Path>, emit: EmitMode) -> Result<()> {
    use anyhow::Context;
    use std::fs;
    
    let src = fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;

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
            let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name)
                .map_err(|e| anyhow::anyhow!(e))?;
            let stub = velox_sfc::to_stub_rs(&sfc, name);
            let indented = render_fn
                .lines()
                .map(|l| format!("    {}", l))
                .collect::<Vec<_>>()
                .join("\n");
            if let Some(pos) = stub.rfind("\n}\n") {
                let before = &stub[..pos+1];
                let after = &stub[pos+1..];
                code.push_str(before);
                code.push_str("\n");
                code.push_str(&indented);
                code.push_str("\n");
                code.push_str(after);
            } else {
                code.push_str(&stub);
                code.push_str("\n");
                code.push_str(&render_fn);
                code.push_str("\n");
            }
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

/// Re-export commands
pub use commands::*;
