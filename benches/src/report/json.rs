use std::fs;
use std::path::Path;

use super::Report;

/// Writes the report as pretty JSON, which is the machine-readable form of exactly what the HTML
/// page renders. Useful for diffing two runs outside criterion, or feeding CI.
pub fn write(report: &Report, path: &Path) -> std::io::Result<()>
{
    let json = serde_json::to_string_pretty(report).expect("Report must always be serialisable");
    if let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, json)
}
