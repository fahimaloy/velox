use anyhow::{Context, Result};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Build the current project via cargo build.
pub fn build_current(release: bool) -> Result<()> {
    let mut args = vec!["build"];
    if release {
        args.push("--release");
    }
    let status = Command::new("cargo").args(&args).status()?;
    if !status.success() {
        anyhow::bail!("project build failed")
    }
    Ok(())
}

/// Build an app package via cargo build -p <pkg>.
pub fn build_app(pkg: &str, release: bool) -> Result<()> {
    let mut args = vec!["build", "-p", pkg];
    if release {
        args.push("--release");
    }
    let status = Command::new("cargo").args(&args).status()?;
    if !status.success() {
        anyhow::bail!("app build failed")
    }
    Ok(())
}

/// Sanitize a component name for use as a Rust module identifier.
fn sanitize_mod_name(name: &str) -> String {
    name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '_', "_")
        .to_lowercase()
}

/// Recursively compile a .vx file and all its imported component dependencies to Rust.
/// Returns a list of (original_component_name, sanitized_module_name) for all compiled components.
fn compile_component_tree(
    vx_file: &Path,
    out_dir: &Path,
    visited: &mut HashSet<String>,
) -> Result<Vec<(String, String)>> {
    let name = vx_file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("component");
    let mod_name = sanitize_mod_name(name);

    // Avoid circular imports
    let canonical = vx_file
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", vx_file.display()))?
        .to_string_lossy()
        .to_string();
    if visited.contains(&canonical) {
        return Ok(vec![(name.to_string(), mod_name)]);
    }
    visited.insert(canonical.clone());

    let src = fs::read_to_string(vx_file)
        .with_context(|| format!("failed to read {}", vx_file.display()))?;

    let sfc = velox_sfc::parse_sfc(&src).map_err(|e| anyhow::anyhow!(e))?;

    // Parse script for component imports
    let mut resolver = velox_sfc::ComponentResolver::new(
        vx_file
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf(),
    );

    if let Some(script_setup) = &sfc.script_setup {
        resolver.parse_imports(&script_setup.content);
    }

    // First, recursively compile all imported components
    // Collect all descendant modules (children first, then grandchildren, etc.)
    let mut descendant_modules: Vec<(String, String)> = Vec::new();
    let mut seen_files: HashSet<String> = HashSet::new();
    seen_files.insert(canonical.clone());

    for comp_name in resolver.component_names() {
        if let Some(import) = resolver.get_import(&comp_name) {
            let source_path = if Path::new(&import.source).is_absolute() {
                PathBuf::from(&import.source)
            } else {
                vx_file.parent().unwrap().join(&import.source)
            };

            if source_path.exists() {
                let child_canonical = source_path
                    .canonicalize()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !seen_files.contains(&child_canonical) {
                    seen_files.insert(child_canonical);
                    let child_modules = compile_component_tree(&source_path, out_dir, visited)?;
                    for (cname, mname) in &child_modules {
                        if !descendant_modules
                            .iter()
                            .any(|(c, m)| c == cname && m == mname)
                        {
                            descendant_modules.push((cname.clone(), mname.clone()));
                        }
                    }
                }
            } else {
                eprintln!(
                    "[velox] Warning: component file not found: {} (imported as {})",
                    source_path.display(),
                    import.source
                );
            }
        }
    }

    let tpl_src = sfc
        .template
        .as_ref()
        .map(|t| t.content.as_str())
        .unwrap_or("");

    let render_fn = velox_sfc::compile_template_to_rs(tpl_src, name, Some(&resolver))
        .map_err(|e| anyhow::anyhow!(e))?;

    // Generate component stub
    let mut stub = velox_sfc::to_stub_rs(&sfc, name);

    // Build module declarations for descendant components.
    // Each unique descendant component has its own .rs file in out_dir.
    // We declare them as submodules using #[path] attributes.
    // We also create `pub use` aliases so the generated code can reference
    // components by their original name (e.g., MyButton) instead of sanitized name.
    if !descendant_modules.is_empty() {
        let mut seen_mods: HashSet<String> = HashSet::new();
        let mut mod_decls = Vec::new();
        let mut aliases = Vec::new();

        for (comp_name, mod_file) in &descendant_modules {
            // Declare the module (only once per unique .rs file)
            if seen_mods.insert(mod_file.clone()) {
                mod_decls.push(format!(
                    "    #[path = \"{}.rs\"]\n    pub mod {};",
                    mod_file, mod_file
                ));
            }
            // Create alias so {comp_name}::render() resolves to {mod_file}::render()
            if comp_name != mod_file {
                aliases.push(format!("    pub use {} as {};", mod_file, comp_name));
            }
        }

        let module_block = if aliases.is_empty() {
            format!("\n{}\n", mod_decls.join("\n"))
        } else {
            format!("\n{}\n\n{}\n", mod_decls.join("\n"), aliases.join("\n"))
        };

        if let Some(pos) = stub.find("\npub const STYLE") {
            stub.insert_str(pos, &module_block);
        }
    }

    let indented = render_fn
        .lines()
        .map(|l| format!("    {}", l))
        .collect::<Vec<_>>()
        .join("\n");

    let mut code = String::new();
    if let Some(pos) = stub.rfind("\n}\n") {
        let before = &stub[..pos + 1];
        let after = &stub[pos + 1..];
        code.push_str(before);
        code.push('\n');
        code.push_str(&indented);
        code.push('\n');
        code.push_str(after);
    } else {
        code.push_str(&stub);
        code.push('\n');
        code.push_str(&render_fn);
        code.push('\n');
    }

    let out_path = out_dir.join(format!("{}.rs", mod_name));
    fs::write(&out_path, &code)?;

    println!("[velox] Generated: {}", out_path.display());

    // Return all modules including self
    let mut result = descendant_modules;
    result.push((name.to_string(), mod_name));
    Ok(result)
}

/// Build a .vx file to Rust, including all imported component dependencies.
pub fn build_vx(input: &Path, out_dir: Option<&Path>) -> Result<()> {
    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("target/velox-gen"));
    fs::create_dir_all(&out_dir)?;

    let mut visited = HashSet::new();
    compile_component_tree(input, &out_dir, &mut visited)?;

    Ok(())
}
