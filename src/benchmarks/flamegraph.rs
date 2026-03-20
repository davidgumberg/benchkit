use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone)]
pub struct Flamegrapher {
    /// Directory where flamegraph.svg files should be stored
    output_dir: PathBuf,
    /// Additional flamegraph record options (defaults to standard profiling options)
    flamegraph_options: Vec<String>,
}

impl Flamegrapher {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            flamegraph_options: vec![
                "-F".to_string(),
                "99".to_string(), // Sample at 99Hz
            ],
        }
    }

    pub fn builder(output_dir: PathBuf) -> FlamegrapherBuilder {
        FlamegrapherBuilder::new(output_dir)
    }

    pub fn validate_flamegraph() -> Result<()> {
        let output = Command::new("flamegraph")
            .arg("--help")
            .output()
            .context(
                "Failed to run flamegraph. Install with: cargo install flamegraph"
            )?;

        if !output.status.success() {
            anyhow::bail!(
                "flamegraph command not functional. Install with: cargo install flamegraph"
            );
        }

        debug!("flamegraph command found and available");

        if let Some(msg) = Self::check_perf_paranoid() {
            warn!("{}", msg);
        }

        Ok(())
    }

    /// Construct a flamegraph record command that wraps the given command
    ///
    /// Returns a command vector: ["flamegraph", ...options..., "-o", "flamegraph.svg", "--", "original", "command"]
    pub fn wrap_command(&self, original_command: &str) -> Result<(Vec<String>, PathBuf)> {
        // Generate the flamegraph.svg output path
        let flamegraph_svg_path = self.output_dir.join("flamegraph.svg");

        // Ensure output directory exists
        std::fs::create_dir_all(&self.output_dir).with_context(|| {
            format!(
                "Failed to create flamegraph output directory: {}",
                self.output_dir.display()
            )
        })?;

        let mut flamegraph_cmd = vec!["flamegraph".to_string()];
        flamegraph_cmd.extend(self.flamegraph_options.clone());
        flamegraph_cmd.push("-o".to_string());
        flamegraph_cmd.push(flamegraph_svg_path.to_string_lossy().to_string());
        // Add separator before actual command
        flamegraph_cmd.push("--".to_string());
        flamegraph_cmd.push(original_command.to_string());

        debug!("Constructed flamegraph command: {:?}", flamegraph_cmd);
        debug!("Flamegraph data will be written to: {}", flamegraph_svg_path.display());

        Ok((flamegraph_cmd, flamegraph_svg_path))
    }

    pub fn get_flamegraph_svg_path(&self) -> PathBuf {
        self.output_dir.join("flamegraph.svg")
    }

    /// Verify that flamegraph.svg was created and move it to the final location if needed
    ///
    /// This is called after command execution to ensure the flamegraph.svg file
    /// is in the expected location within the benchmark output directory
    pub fn finalize_flamegraph_svg(&self) -> Result<bool> {
        let expected_path = self.get_flamegraph_svg_path();

        if expected_path.exists() {
            let file_size = std::fs::metadata(&expected_path)
                .context("Failed to read flamegraph.svg metadata")?
                .len();

            if file_size == 0 {
                warn!(
                    "flamegraph.svg file exists but is empty at: {}",
                    expected_path.display()
                );
                return Ok(false);
            }

            info!(
                "flamegraph.svg created successfully: {} ({} bytes)",
                expected_path.display(),
                file_size
            );
            Ok(true)
        } else {
            warn!(
                "flamegraph.svg file was not created at expected location: {}",
                expected_path.display()
            );

            // Check if flamegraph.svg was created in current working directory (fallback)
            let cwd_flamegraph = Path::new("flamegraph.svg");
            if cwd_flamegraph.exists() {
                warn!("Found flamegraph.svg in current directory, moving to output directory");
                std::fs::rename(cwd_flamegraph, &expected_path)
                    .context("Failed to move flamegraph.svg from current directory")?;
                return Ok(true);
            }

            Ok(false)
        }
    }
    /// Read `/proc/sys/kernel/perf_event_paranoid` and return a warning
    /// message if the value is too restrictive for flamegraph profiling.
    ///
    /// Returns `None` on non-Linux or when the level is permissive enough.
    pub fn check_perf_paranoid() -> Option<String> {
        let content = std::fs::read_to_string("/proc/sys/kernel/perf_event_paranoid").ok()?;
        let level: i32 = content.trim().parse().ok()?;

        if level >= 2 {
            Some(format!(
                "kernel.perf_event_paranoid = {} (level >= 2 blocks CPU profiling for \
                 unprivileged users).\n  \
                 Flamegraph generation will likely fail silently.\n  \
                 Fix with:  sudo sysctl kernel.perf_event_paranoid=1\n  \
                 Or permanently in /etc/sysctl.d/:\n    \
                 echo 'kernel.perf_event_paranoid=1' | sudo tee /etc/sysctl.d/99-perf.conf\n    \
                 sudo sysctl --system",
                level
            ))
        } else {
            None
        }
    }
}

/// Builder for FlamegraphInstrumentor with custom options
pub struct FlamegrapherBuilder {
    output_dir: PathBuf,
    flamegraph_options: Vec<String>,
}

impl FlamegrapherBuilder {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            flamegraph_options: vec![
                "-F".to_string(),
                "99".to_string(),
            ],
        }
    }

    /// Set custom flamegraph record options
    pub fn flamegraph_options(mut self, options: Vec<String>) -> Self {
        self.flamegraph_options = options;
        self
    }

    /// Add additional flamegraph record options
    pub fn add_flamegraph_option(mut self, option: String) -> Self {
        self.flamegraph_options.push(option);
        self
    }

    pub fn sampling_frequency(mut self, freq: u32) -> Self {
        // Is there a -F at some pos?
        if let Some(pos) = self.flamegraph_options.iter().position(|opt| opt == "-F") {
            // Drop the item at pos
            self.flamegraph_options.remove(pos);
            // Drop the item that now shifted into pos.
            if pos < self.flamegraph_options.len() {
                self.flamegraph_options.remove(pos);
            }
        }

        self.flamegraph_options.push("-F".to_string());
        self.flamegraph_options.push(freq.to_string());
        self
    }

    pub fn build(self) -> Flamegrapher {
        Flamegrapher {
            output_dir: self.output_dir,
            flamegraph_options: self.flamegraph_options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_flamegraph_instrumentor_creation() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = Flamegrapher::new(temp_dir.path().to_path_buf());

        assert_eq!(instrumentor.output_dir, temp_dir.path());
        assert!(!instrumentor.flamegraph_options.is_empty());
    }

    #[test]
    fn test_wrap_command() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = Flamegrapher::new(temp_dir.path().to_path_buf());

        let (flamegraph_cmd, flamegraph_svg_path) = instrumentor.wrap_command("bitcoind -version").unwrap();

        assert_eq!(flamegraph_cmd[0], "flamegraph");
        assert!(flamegraph_cmd.contains(&"-F".to_string()));
        assert!(flamegraph_cmd.contains(&"99".to_string()));
        assert!(flamegraph_cmd.contains(&"--".to_string()));
        assert!(flamegraph_cmd.contains(&"bitcoind -version".to_string()));

        assert_eq!(flamegraph_svg_path, temp_dir.path().join("flamegraph.svg"));
    }

    #[test]
    fn test_builder() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = Flamegrapher::builder(temp_dir.path().to_path_buf())
            .sampling_frequency(50)
            .add_flamegraph_option("--no-inherit".to_string())
            .build();

        assert!(instrumentor.flamegraph_options.contains(&"-F".to_string()));
        assert!(instrumentor.flamegraph_options.contains(&"50".to_string()));
        assert!(instrumentor
            .flamegraph_options
            .contains(&"--no-inherit".to_string()));
        assert!(!instrumentor.flamegraph_options.contains(&"99".to_string()));
    }

    #[test]
    fn test_get_flamegraph_svg_path() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = Flamegrapher::new(temp_dir.path().to_path_buf());

        let expected_path = temp_dir.path().join("flamegraph.svg");
        assert_eq!(instrumentor.get_flamegraph_svg_path(), expected_path);
    }
}
