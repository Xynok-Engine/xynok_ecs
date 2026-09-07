use std::fs;
use std::path::Path;

use super::Report;

/// The page is one file with the data baked in, so it opens from disk with no server and no
/// fetch. The placeholder is replaced with the same JSON `json::write` produces.
const TEMPLATE: &str = include_str!("report.html.tmpl");
const PLACEHOLDER: &str = "__BENCH_DATA_JSON__";

pub fn write(report: &Report, path: &Path) -> std::io::Result<()>
{
    let json = serde_json::to_string(report).expect("Report must always be serialisable");
    let html = TEMPLATE.replace(PLACEHOLDER, &json);
    if let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, html)
}
