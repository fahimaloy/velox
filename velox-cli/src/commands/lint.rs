use anyhow::Result;
use std::fs;
use std::path::Path;

/// Lint a single .vx file. Returns `true` if the file failed to parse.
fn lint_file_error(path: &Path) -> bool {
    let content = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            println!("❌ {} - Read error: {}", path.display(), e);
            return true;
        }
    };
    match velox_sfc::parse_sfc(&content) {
        Ok(_) => false,
        Err(e) => {
            println!("❌ {} - Parse error: {}", path.display(), e);
            true
        }
    }
}

/// Normalize a source string: strip trailing whitespace on every line and
/// ensure the file ends with exactly one trailing newline.
fn normalize_source(src: &str) -> String {
    let mut lines: Vec<&str> = src.split('\n').map(|l| l.trim_end()).collect();
    // Drop any trailing empty lines so we end with a single final newline.
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

/// Apply auto-fixes to a single .vx file. Returns `true` if a hard error
/// occurred (read failure, parse failure, write failure). A file that is
/// successfully fixed (or already clean) is NOT an error.
///
/// Parse errors are not auto-fixable, so non-parseable files are reported as
/// errors and left untouched.
fn fix_file(path: &Path) -> bool {
    let original = match fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            println!("❌ {} - Read error: {}", path.display(), e);
            return true;
        }
    };

    if let Err(e) = velox_sfc::parse_sfc(&original) {
        println!("❌ {} - Parse error: {}", path.display(), e);
        return true;
    }

    let fixed = normalize_source(&original);
    if fixed == original {
        println!("✅ {}", path.display());
        return false;
    }

    match fs::write(path, &fixed) {
        Ok(()) => {
            println!("🔧 {} - fixed", path.display());
            false
        }
        Err(e) => {
            println!("❌ {} - Write error: {}", path.display(), e);
            true
        }
    }
}

/// Lint (and optionally auto-fix) every `.vx` file under `dir`, recursively.
/// Returns `Err` if any file failed its parse check.
pub fn lint_directory_fix(dir: &Path, fix: bool) -> Result<()> {
    fn walk(dir: &Path, file_count: &mut usize, error_count: &mut usize, fix: bool) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                walk(&path, file_count, error_count, fix)?;
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) == Some("vx") {
                *file_count += 1;
                let errored = if fix {
                    fix_file(&path)
                } else {
                    lint_file_error(&path)
                };
                if errored {
                    *error_count += 1;
                }
            }
        }
        Ok(())
    }

    let mut file_count = 0usize;
    let mut error_count = 0usize;
    walk(dir, &mut file_count, &mut error_count, fix)?;

    if file_count == 0 {
        println!("⚠️  No .vx files found in {}", dir.display());
        return Ok(());
    }

    let label = if fix { "Lint+fix" } else { "Lint" };
    println!("\n📊 {label} results: {} files, {} errors", file_count, error_count);

    if error_count > 0 {
        anyhow::bail!("Lint failed with {} errors", error_count);
    }
    Ok(())
}

/// Lint a single .vx file (no auto-fix).
pub fn lint_file(path: &Path) -> Result<()> {
    if lint_file_error(path) {
        Err(anyhow::anyhow!("Lint failed"))
    } else {
        Ok(())
    }
}

/// Auto-fix a single .vx file. Returns `Err` if a hard error occurred
/// (read/parse/write failure); successfully-fixed files return `Ok`.
pub fn fix_file_single(path: &Path) -> Result<()> {
    if fix_file(path) {
        Err(anyhow::anyhow!("Lint failed"))
    } else {
        Ok(())
    }
}

/// Recursively lint all `.vx` files under `dir` (no auto-fix).
pub fn lint_directory(dir: &Path) -> Result<()> {
    lint_directory_fix(dir, false)
}