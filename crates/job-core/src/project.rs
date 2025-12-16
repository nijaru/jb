use std::path::PathBuf;
use std::process::Command;

pub fn detect_project(cwd: &PathBuf) -> PathBuf {
    if let Ok(output) = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        && output.status.success()
            && let Ok(path) = String::from_utf8(output.stdout) {
                return PathBuf::from(path.trim());
            }
    cwd.clone()
}
