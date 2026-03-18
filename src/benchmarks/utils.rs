use std::net::TcpListener;
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

fn get_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Build base bitcoind command arguments that are common across all invocations
pub fn build_bitcoind_base_args(network: &str, datadir: &Path, connect: &str) -> Vec<String> {
    let mut args = vec![
        format!("-chain={}", network),
        format!("-port={}", get_available_port()),
        format!("-rpcport={}", get_available_port()),
        format!("-datadir={}", datadir.display()),
    ];

    if !connect.is_empty() {
        args.push(format!("-connect={}", connect));
    }

    args
}

/// Build the full benchmark command with parameter substitution
pub fn build_benchmark_command(
    bin_dir: &Path,
    commit_placeholder: &str,
    network: &str,
    datadir: &Path,
    connect: &str,
    command_template: &str,
) -> String {
    let bitcoind_path = format!("{}/bitcoind-{}", bin_dir.display(), commit_placeholder);
    let base_args = build_bitcoind_base_args(network, datadir, connect);
    let base_args_str = base_args.join(" ");
    command_template.replace("bitcoind", &format!("{} {}", bitcoind_path, base_args_str))
}
