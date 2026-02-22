use std::path::{Path, PathBuf};
use std::process::Command;

#[must_use]
pub fn detect_project(cwd: &Path) -> PathBuf {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        && output.status.success()
        && let Ok(path) = String::from_utf8(output.stdout)
    {
        return PathBuf::from(path.trim());
    }
    cwd.to_path_buf()
}
