use anyhow::Result;
use std::path::Path;

/// Lint .vx files for syntax errors
pub fn lint_file(path: &Path) -> Result<()> {
    let content = std::fs::read_to_string(path)?;
    match velox_sfc::parse_sfc(&content) {
        Ok(_) => {
            println!("✅ {}", path.display());
            Ok(())
        }
        Err(e) => {
            println!("❌ {} - {}", path.display(), e);
            Err(anyhow::anyhow!("Parse error: {}", e))
        }
    }
}

/// Lint all .vx files in a directory
pub fn lint_directory(dir: &Path) -> Result<()> {
    let mut file_count = 0;
    let mut error_count = 0;

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.extension().and_then(|s| s.to_str()) == Some("vx") {
            file_count += 1;
            if lint_file(&path).is_err() {
                error_count += 1;
            }
        }
    }

    println!("\n📊 Lint results: {} files, {} errors", file_count, error_count);

    if error_count > 0 {
        Err(anyhow::anyhow!("Lint failed with {} errors", error_count))
    } else {
        Ok(())
    }
}
