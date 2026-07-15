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

/// Result of compiling a .vx component tree.
pub struct CompileResult {
    /// List of (original_component_name, sanitized_module_name) for all compiled components.
    pub modules: Vec<(String, String)>,
    /// List of all .vx files that were read during compilation (for rerun-if-changed).
    pub vx_files: Vec<PathBuf>,
}

/// Recursively compile a .vx file and all its imported component dependencies to Rust.
/// Returns module information and the list of all .vx files read.
fn compile_component_tree(
    vx_file: &Path,
    out_dir: &Path,
    visited: &mut HashSet<String>,
    all_vx_files: &mut Vec<PathBuf>,
    is_root: bool,
    child_styles: &mut Vec<String>,
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

    // Track this .vx file for rerun-if-changed
    all_vx_files.push(vx_file.to_path_buf());

    let src = fs::read_to_string(vx_file)
        .with_context(|| format!("failed to read {}", vx_file.display()))?;

    let sfc = velox_sfc::parse_sfc(&src)
        .map_err(|e| anyhow::anyhow!("SFC parse error in {}: {}", vx_file.display(), e))?;

    // Parse script for component imports, using the .vx file's parent directory as base path
    let base_path = vx_file
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .to_path_buf();
    let mut resolver = velox_sfc::ComponentResolver::new(base_path.clone());

    if let Some(script_setup) = &sfc.script_setup {
        resolver.parse_imports(&script_setup.content);
    }

    // Collect this component's <style> for merging into the root app STYLE so
    // child-component CSS actually applies (the root only parses app::STYLE).
    if !is_root {
        if let Some(style) = &sfc.style {
            child_styles.push(style.content.clone());
        }
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
                    let child_modules = compile_component_tree(
                        &source_path,
                        out_dir,
                        visited,
                        all_vx_files,
                        false,
                        child_styles,
                    )?;
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
                log::warn!(
                    "component file not found: {} (imported as {})",
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

    let render_fn =
        velox_sfc::compile_template_to_rs(tpl_src, name, Some(&resolver)).map_err(|e| {
            anyhow::anyhow!("template compilation error in {}: {}", vx_file.display(), e)
        })?;

    // Generate component stub, passing the base path for correct import resolution.
    // Use unwrapped mode so the output is the module body (no `pub mod {name} { ... }`
    // wrapper). This prevents double-nesting when child components are included via
    // #[path] attributes.
    let mut stub = velox_sfc::to_stub_rs_unwrapped(&sfc, name, Some(&base_path));

    // Build module declarations for descendant components.
    // Each unique descendant component has its own .rs file in out_dir.
    // We declare them as submodules using #[path] attributes with ABSOLUTE paths
    // because the parent module may be included via include!() which changes the
    // resolution context.
    let mut module_block = String::new();
    if !descendant_modules.is_empty() {
        let mut seen_mods: HashSet<String> = HashSet::new();
        let mut mod_decls = Vec::new();
        let mut aliases = Vec::new();

        for (comp_name, mod_file) in &descendant_modules {
            if seen_mods.insert(mod_file.clone()) {
                let abs_path = out_dir
                    .join(format!("{}.rs", mod_file))
                    .display()
                    .to_string();
                // No indentation here — we'll add it when wrapping in pub mod {name} {}
                mod_decls.push(format!("#[path = \"{}\"]\npub mod {};", abs_path, mod_file));
            }
            if comp_name != mod_file {
                aliases.push(format!("pub use {} as {};", mod_file, comp_name));
            }
        }

        module_block.push('\n');
        for decl in &mod_decls {
            module_block.push_str(decl);
            module_block.push('\n');
        }
        if !aliases.is_empty() {
            module_block.push('\n');
            for alias in &aliases {
                module_block.push_str(alias);
                module_block.push('\n');
            }
        }
    }

    // Insert module declarations after the comment header line in the stub
    if !module_block.is_empty() {
        if let Some(pos) = stub.find('\n') {
            stub.insert_str(pos + 1, &module_block);
        } else {
            stub.push_str(&module_block);
        }
    }

    // Merge child-component styles into the root app's STYLE constant so the
    // renderer applies them (main.rs only parses `app::STYLE`).
    if is_root && !child_styles.is_empty() {
        let merged = child_styles.join("\n");
        if let Some(pos) = stub.find("pub const STYLE") {
            // Insert just before the CLOSING `"#` of the raw string literal.
            // (Raw string delimiters are `r#" ... "#`; the opening is `r#"`,
            // the closing is `"#` — search for the closing to avoid matching
            // the opening.)
            if let Some(end) = stub[pos..].rfind("\"#") {
                stub.insert_str(pos + end, &format!("\n{}", merged));
            }
        }
    }

    // Indent render_fn lines by 4 spaces so they sit inside pub mod {name} { }
    // (or at module-level for unwrapped child components)
    let render_indent = if is_root { "    " } else { "" };
    let indented = render_fn
        .lines()
        .map(|l| format!("{render_indent}{l}"))
        .collect::<Vec<_>>()
        .join("\n");

    let mut code = String::new();
    if is_root {
        // Root component: wrap in `pub mod {name} { ... }` so include!() in main.rs works
        code.push_str(&format!("pub mod {} {{\n", mod_name));
        // The stub is the module body without a wrapper; indent it by 4 spaces
        for line in stub.lines() {
            if line.is_empty() {
                code.push('\n');
            } else {
                code.push_str(&format!("    {line}\n"));
            }
        }
        code.push('\n');
        code.push_str(&indented);
        code.push('\n');
        code.push_str("}\n");
    } else {
        // Child component: just the module body (no pub mod wrapper).
        // The #[path] attribute in the parent's module declaration already
        // creates the module — the file content becomes the module body.
        code.push_str(&stub);
        code.push('\n');
        code.push('\n');
        code.push_str(&indented);
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
/// Returns a CompileResult with module info and all .vx files read (for rerun-if-changed).
pub fn build_vx(input: &Path, out_dir: Option<&Path>) -> Result<CompileResult> {
    let out_dir = out_dir
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("target/velox-gen"));
    fs::create_dir_all(&out_dir)?;

    let mut visited = HashSet::new();
    let mut all_vx_files = Vec::new();
    let mut child_styles: Vec<String> = Vec::new();
    let modules = compile_component_tree(input, &out_dir, &mut visited, &mut all_vx_files, true, &mut child_styles)?;

    Ok(CompileResult {
        modules,
        vx_files: all_vx_files,
    })
}
