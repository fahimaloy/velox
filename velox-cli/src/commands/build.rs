use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use std::process::Command;

/// Build an app package via cargo build
pub fn build_app(pkg: &str, release: bool) -> Result<()> {
    let mut args = vec!["build", "-p", pkg];
    if release { args.push("--release"); }
    let status = Command::new("cargo").args(&args).status()?;
    if !status.success() { anyhow::bail!("app build failed") }
    Ok(())
}

/// Build a .vx file to Rust
pub fn build_vx(input: &Path, out_dir: Option<&Path>) -> Result<()> {
    let src = fs::read_to_string(input)
        .with_context(|| format!("failed to read {}", input.display()))?;
    
    let sfc = velox_sfc::parse_sfc(&src).map_err(|e| anyhow::anyhow!(e))?;
    
    let name = input
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component");
    
    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");
    
    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name)
        .map_err(|e| anyhow::anyhow!(e))?;
    
    let mut stub = velox_sfc::to_stub_rs(&sfc, name);
    
    let indented = render_fn
        .lines()
        .map(|l| format!("    {}", l))
        .collect::<Vec<_>>()
        .join("\n");
    
    let mut code = String::new();
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
    
    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("target/velox-gen"));
    fs::create_dir_all(&out_dir)?;
    
    let out_path = out_dir.join(format!("{}.rs", name));
    fs::write(&out_path, code)?;
    
    println!("Generated: {}", out_path.display());
    Ok(())
}
