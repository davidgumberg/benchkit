use std::path::{Path, PathBuf};

/// Check if a binary exists for a given commit
pub fn binary_exists(bin_dir: &Path, commit: &str) -> bool {
    let binary_path = get_binary_path(bin_dir, commit);
    binary_path.exists()
}

/// Get the full path to a binary for a given commit
pub fn get_binary_path(bin_dir: &Path, commit: &str) -> PathBuf {
    bin_dir.join(format!("bitcoind-{commit}"))
}

/// Check if all required binaries exist and return missing ones
pub fn check_binaries_exist(
    bin_dir: &Path,
    commits: &[String],
) -> Result<(), Vec<(String, PathBuf)>> {
    let mut missing_binaries = Vec::new();

    for commit in commits {
        let binary_path = get_binary_path(bin_dir, commit);
        if !binary_path.exists() {
            missing_binaries.push((commit.clone(), binary_path));
        }
    }

    if missing_binaries.is_empty() {
        Ok(())
    } else {
        Err(missing_binaries)
    }
}
