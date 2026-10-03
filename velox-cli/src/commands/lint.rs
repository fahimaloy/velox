use anyhow::Result;
use std::fs;
use std::path::Path;

/// Print reactive-idiom warnings (Cell/RefCell usage in `<script>`) for a
/// parsed SFC. These are advisory style warnings: they never count as lint
/// errors and never fail the command — only parse errors do.
fn print_script_warnings(sfc: &velox_sfc::Sfc, path: &Path) {
    for block in [sfc.script_setup.as_ref(), sfc.script.as_ref()]
        .into_iter()
        .flatten()
    {
        for warning in velox_sfc::lint_script(&block.content) {
            println!("⚠️  {} - {}", path.display(), warning);
        }
    }
}

/// Warn about CSS declarations that velox parses but never renders.
///
/// A property listed in `ComputedStyle::PARSED_BUT_UNRENDERED` has a
/// `set_property` arm but no reader on the live path, so authoring it looks
/// correct and does nothing — `visibility: hidden` hides nothing.
///
/// This reports **only** table members, never "anything unmatched": the
/// cascade filters unknown declarations out silently, so flagging genuinely
/// unknown properties would contradict the spec and would fire on every vendor
/// prefix, custom property and deliberately-declined property.
///
/// The parsed `Sfc` already carries the `<style>` block as a raw string; this is
/// its first consumer. `StyleBlock` carries no source offset, so a true line
/// number is not recoverable — we report file + rule index rather than
/// approximating a line that may point at unrelated code.
fn print_style_warnings(sfc: &velox_sfc::Sfc, path: &Path) {
    let Some(block) = sfc.style.as_ref() else {
        return;
    };
    // `Stylesheet::parse` skips malformed rules rather than failing, so a
    // broken stylesheet yields fewer rules and never panics.
    let sheet = velox_style::Stylesheet::parse(&block.content);
    for (i, rule) in sheet.rules.iter().enumerate() {
        // `decls` is a HashMap: sort so warnings are stable run to run.
        let mut props: Vec<&str> = rule.decls.keys().map(String::as_str).collect();
        props.sort_unstable();
        for prop in props {
            if let Some((_, why)) = velox_style::ComputedStyle::PARSED_BUT_UNRENDERED
                .iter()
                .find(|(name, _)| *name == prop)
            {
                println!(
                    "⚠️  {} - <style> rule {}: `{}` is parsed but never rendered ({why})",
                    path.display(),
                    i + 1,
                    prop
                );
            }
        }
    }
}

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
        Ok(sfc) => {
            print_script_warnings(&sfc, path);
            print_style_warnings(&sfc, path);
            false
        }
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

    let sfc = match velox_sfc::parse_sfc(&original) {
        Ok(sfc) => sfc,
        Err(e) => {
            println!("❌ {} - Parse error: {}", path.display(), e);
            return true;
        }
    };

    print_script_warnings(&sfc, path);
    print_style_warnings(&sfc, path);

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
    println!(
        "\n📊 {label} results: {} files, {} errors",
        file_count, error_count
    );

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
