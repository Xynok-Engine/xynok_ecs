use std::fs;
use std::path::Path;

use super::BenchResult;

const TEMPLATE: &str = include_str!("report.html.tmpl");

pub fn write(results: &[BenchResult], path: &Path) -> std::io::Result<()>
{
    let json = serde_json::to_string(results).expect("BenchResult must always be serializable");
    let html = TEMPLATE.replace("__BENCH_DATA_JSON__", &json);
    if let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, html)
}
