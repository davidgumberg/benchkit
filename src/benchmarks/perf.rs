use anyhow::{Context, Result};
use log::{debug, info, warn};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Manages perf instrumentation for benchmark commands
///
/// The PerfInstrumentor handles:
/// - Constructing perf record commands with appropriate options
/// - Validating that perf is available on the system
/// - Managing perf.data file output paths
/// - Moving perf.data files to the correct output directory
#[derive(Debug, Clone)]
pub struct PerfInstrumentor {
    /// Directory where perf.data files should be stored
    output_dir: PathBuf,
    /// Additional perf record options (defaults to standard profiling options)
    perf_options: Vec<String>,
}

impl PerfInstrumentor {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            perf_options: vec![
                "-g".to_string(), // Enable call graph recording
                "--call-graph".to_string(),
                "fp".to_string(), // Use frame pointers (requires building with -fno-omit-frame-pointer)
                "-F".to_string(),
                "99".to_string(), // Sample at 99Hz
            ],
        }
    }

    pub fn builder(output_dir: PathBuf) -> PerfInstrumentorBuilder {
        PerfInstrumentorBuilder::new(output_dir)
    }

    pub fn validate_perf_available() -> Result<()> {
        #[cfg(not(target_os = "linux"))]
        {
            anyhow::bail!("perf instrumentation is only supported on Linux");
        }

        #[cfg(target_os = "linux")]
        {
            // Check if perf command exists
            let output = Command::new("which")
                .arg("perf")
                .output()
                .context("Failed to check if perf is installed")?;

            if !output.status.success() {
                anyhow::bail!(
                    "perf command not found. Please install perf using your package manager.\n\
                    On Ubuntu/Debian: sudo apt install linux-perf\n\
                    On RHEL/CentOS: sudo yum install perf\n\
                    On Arch: sudo pacman -S perf"
                );
            }

            debug!("perf command found and available");
            Ok(())
        }
    }

    /// Construct a perf record command that wraps the given command
    ///
    /// Returns a command vector: ["perf", "record", ...options..., "-o", "perf.data", "--", "original", "command"]
    pub fn wrap_command(&self, original_command: &str) -> Result<(Vec<String>, PathBuf)> {
        // Generate the perf.data output path
        let perf_data_path = self.output_dir.join("perf.data");

        // Ensure output directory exists
        std::fs::create_dir_all(&self.output_dir).with_context(|| {
            format!(
                "Failed to create perf output directory: {}",
                self.output_dir.display()
            )
        })?;

        let mut perf_cmd = vec!["perf".to_string(), "record".to_string()];
        perf_cmd.extend(self.perf_options.clone());
        perf_cmd.push("-o".to_string());
        perf_cmd.push(perf_data_path.to_string_lossy().to_string());
        // Add separator before actual command
        perf_cmd.push("--".to_string());
        // Add the original command (let the shell handle parsing)
        perf_cmd.push("sh".to_string());
        perf_cmd.push("-c".to_string());
        perf_cmd.push(original_command.to_string());

        debug!("Constructed perf command: {:?}", perf_cmd);
        debug!("Perf data will be written to: {}", perf_data_path.display());

        Ok((perf_cmd, perf_data_path))
    }

    pub fn get_perf_data_path(&self) -> PathBuf {
        self.output_dir.join("perf.data")
    }

    /// Verify that perf.data was created and move it to the final location if needed
    ///
    /// This is called after command execution to ensure the perf.data file
    /// is in the expected location within the benchmark output directory
    pub fn finalize_perf_data(&self) -> Result<bool> {
        let expected_path = self.get_perf_data_path();

        if expected_path.exists() {
            let file_size = std::fs::metadata(&expected_path)
                .context("Failed to read perf.data metadata")?
                .len();

            if file_size == 0 {
                warn!(
                    "perf.data file exists but is empty at: {}",
                    expected_path.display()
                );
                return Ok(false);
            }

            info!(
                "perf.data created successfully: {} ({} bytes)",
                expected_path.display(),
                file_size
            );
            Ok(true)
        } else {
            warn!(
                "perf.data file was not created at expected location: {}",
                expected_path.display()
            );

            // Check if perf.data was created in current working directory (fallback)
            let cwd_perf = Path::new("perf.data");
            if cwd_perf.exists() {
                warn!("Found perf.data in current directory, moving to output directory");
                std::fs::rename(cwd_perf, &expected_path)
                    .context("Failed to move perf.data from current directory")?;
                return Ok(true);
            }

            Ok(false)
        }
    }
}

/// Builder for PerfInstrumentor with custom options
pub struct PerfInstrumentorBuilder {
    output_dir: PathBuf,
    perf_options: Vec<String>,
}

impl PerfInstrumentorBuilder {
    pub fn new(output_dir: PathBuf) -> Self {
        Self {
            output_dir,
            perf_options: vec![
                "-g".to_string(),
                "--call-graph".to_string(),
                "fp".to_string(), // Default to frame pointers for better performance
                "-F".to_string(),
                "99".to_string(),
            ],
        }
    }

    /// Set custom perf record options
    pub fn perf_options(mut self, options: Vec<String>) -> Self {
        self.perf_options = options;
        self
    }

    /// Add additional perf record options
    pub fn add_perf_option(mut self, option: String) -> Self {
        self.perf_options.push(option);
        self
    }

    /// Set sampling frequency (replaces default -F 99)
    pub fn sampling_frequency(mut self, freq: u32) -> Self {
        // Remove existing frequency settings
        self.perf_options.retain(|opt| opt != "-F");
        if let Some(pos) = self.perf_options.iter().position(|opt| opt == "99") {
            self.perf_options.remove(pos);
        }

        self.perf_options.push("-F".to_string());
        self.perf_options.push(freq.to_string());
        self
    }

    pub fn build(self) -> PerfInstrumentor {
        PerfInstrumentor {
            output_dir: self.output_dir,
            perf_options: self.perf_options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_perf_instrumentor_creation() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = PerfInstrumentor::new(temp_dir.path().to_path_buf());

        assert_eq!(instrumentor.output_dir, temp_dir.path());
        assert!(!instrumentor.perf_options.is_empty());
        assert!(instrumentor.perf_options.contains(&"-g".to_string()));
    }

    #[test]
    fn test_wrap_command() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = PerfInstrumentor::new(temp_dir.path().to_path_buf());

        let (perf_cmd, perf_data_path) = instrumentor.wrap_command("bitcoind -version").unwrap();

        assert_eq!(perf_cmd[0], "perf");
        assert_eq!(perf_cmd[1], "record");
        assert!(perf_cmd.contains(&"-g".to_string()));
        assert!(perf_cmd.contains(&"--".to_string()));
        assert!(perf_cmd.contains(&"bitcoind -version".to_string()));

        assert_eq!(perf_data_path, temp_dir.path().join("perf.data"));
    }

    #[test]
    fn test_builder() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = PerfInstrumentor::builder(temp_dir.path().to_path_buf())
            .sampling_frequency(50)
            .add_perf_option("--no-inherit".to_string())
            .build();

        assert!(instrumentor.perf_options.contains(&"-F".to_string()));
        assert!(instrumentor.perf_options.contains(&"50".to_string()));
        assert!(instrumentor
            .perf_options
            .contains(&"--no-inherit".to_string()));
        assert!(!instrumentor.perf_options.contains(&"99".to_string()));
    }

    #[test]
    fn test_get_perf_data_path() {
        let temp_dir = tempdir().unwrap();
        let instrumentor = PerfInstrumentor::new(temp_dir.path().to_path_buf());

        let expected_path = temp_dir.path().join("perf.data");
        assert_eq!(instrumentor.get_perf_data_path(), expected_path);
    }
}
