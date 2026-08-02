use std::fs;
use std::path::Path;

use super::BenchResult;

pub fn write(results: &[BenchResult], path: &Path) -> std::io::Result<()>
{
    let json = serde_json::to_string_pretty(results).expect("BenchResult must always be serializable");
    if let Some(parent) = path.parent()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, json)
}
