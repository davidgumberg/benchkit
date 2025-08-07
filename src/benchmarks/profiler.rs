use anyhow::Result;
use libc;
use log::{debug, info, trace, warn};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

/// Data collected during a single profiling sample point
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileSample {
    /// Seconds elapsed since start of profiling
    pub time: u64,
    /// Total CPU usage as percentage (100% per core)
    pub cpu_usage: f32,
    /// Total memory usage in bytes
    pub memory: u64,
    /// Total virtual memory usage in bytes
    pub virtual_memory: u64,
    /// Total disk read in bytes
    pub disk_read: u64,
    /// Total disk write in bytes
    pub disk_write: u64,
}

/// Results from a profiling session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResult {
    /// The command that was profiled
    pub command: String,
    /// Total duration of the profiling session in seconds
    pub duration: f64,
    /// Process exit code
    pub exit_code: i32,
    /// Samples collected during profiling
    pub samples: Vec<ProfileSample>,
}

/// Builder for Profiler
pub struct ProfilerBuilder {
    /// Output directory path
    output_dir: PathBuf,
    /// Sample interval in seconds
    sample_interval: u64,
    /// CPU cores to bind the process to
    benchmark_cores: Option<String>,
    /// Custom output file name (defaults to "profile_data.json")
    output_filename: Option<String>,
}

impl ProfilerBuilder {
    /// Create a new ProfilerBuilder
    pub fn new(output_dir: &Path) -> Self {
        Self {
            output_dir: output_dir.to_path_buf(),
            sample_interval: 5, // Default 5 second interval
            benchmark_cores: None,
            output_filename: None,
        }
    }

    /// Set the sample interval in seconds
    pub fn sample_interval(mut self, interval: u64) -> Self {
        self.sample_interval = interval;
        self
    }

    /// Set CPU cores to bind the profiled process to
    pub fn benchmark_cores(mut self, cores: Option<String>) -> Self {
        self.benchmark_cores = cores;
        self
    }

    /// Set custom output filename
    pub fn output_filename(mut self, filename: impl Into<String>) -> Self {
        self.output_filename = Some(filename.into());
        self
    }

    /// Build the Profiler instance
    pub fn build(self) -> Result<Profiler> {
        // Create the output directory if it doesn't exist
        if !self.output_dir.exists() {
            std::fs::create_dir_all(&self.output_dir)?;
        }

        // Construct the output path
        let filename = self
            .output_filename
            .unwrap_or_else(|| "profile_data.json".to_string());
        let output_path = self.output_dir.join(filename);

        Ok(Profiler {
            output_path,
            sample_interval: self.sample_interval,
        })
    }
}

/// The profiler that monitors system resources of a process and its children
pub struct Profiler {
    /// Output file path
    output_path: PathBuf,
    /// Sample interval in seconds
    sample_interval: u64,
}

impl Profiler {
    /// Create a new ProfilerBuilder
    pub fn builder(output_dir: &Path) -> ProfilerBuilder {
        ProfilerBuilder::new(output_dir)
    }

    /// Profile an already launched child process
    /// This allows the caller to handle process launching and CPU affinity
    pub fn profile_process(
        &mut self,
        command: &str,
        mut child: std::process::Child,
    ) -> Result<ProfileResult> {
        info!("Profiling process from command: {command}");
        debug!("Will sample every {} seconds", self.sample_interval);

        let parent_pid = Pid::from_u32(child.id());
        debug!("Profiling process with PID: {}", parent_pid.as_u32());

        let start_time = Instant::now();
        let mut samples = Vec::new();
        let mut sys = System::new_all();

        // Main profiling loop with timeout guard for bitcoind stalling
        let mut last_active_time = Instant::now();
        const MAX_INACTIVE_DURATION: Duration = Duration::from_secs(300); // 5 minutes timeout

        while child.try_wait()?.is_none() {
            // Refresh system info to get latest process data
            sys.refresh_all();

            // Check if the process is still running
            if sys.process(parent_pid).is_none() {
                debug!("Process appears to have terminated outside our monitoring");
                break;
            }

            // Collect sample data
            let sample = collect_process_sample(&sys, parent_pid, start_time.elapsed().as_secs());

            trace!(
                "Sample at {}s: CPU: {:.2}%, Memory: {:.2}MB, VMemory: {:.2}MB, Disk R/W: {}/{} bytes",
                sample.time,
                sample.cpu_usage,
                sample.memory as f64 / (1024.0 * 1024.0),
                sample.virtual_memory as f64 / (1024.0 * 1024.0),
                sample.disk_read,
                sample.disk_write
            );

            // If we detect activity, update the last active timestamp
            if sample.cpu_usage > 0.5 || sample.disk_read > 0 || sample.disk_write > 0 {
                last_active_time = Instant::now();
            }

            // Check for potential stalling - if no activity for MAX_INACTIVE_DURATION,
            // terminate the process
            if Instant::now().duration_since(last_active_time) > MAX_INACTIVE_DURATION {
                warn!("Process seems to be stalled (no activity for 5 minutes). Terminating.");

                // Try to terminate gracefully first
                if let Err(e) = child.kill() {
                    warn!("Failed to kill stalled process: {e}");
                    // Try direct kill via system call as fallback
                    unsafe {
                        libc::kill(parent_pid.as_u32() as i32, libc::SIGTERM);
                    }
                }

                // Also try to terminate any child processes
                let pgid = -(parent_pid.as_u32() as i32);
                unsafe {
                    // Send SIGTERM to the process group
                    libc::kill(pgid, libc::SIGTERM);
                }

                break;
            }

            samples.push(sample);
            std::thread::sleep(Duration::from_secs(self.sample_interval));
        }

        let exit_status = child.wait()?;
        let duration = start_time.elapsed().as_secs_f64();
        let exit_code = exit_status.code().unwrap_or(-1);
        let profile_result = ProfileResult {
            command: command.to_string(),
            duration,
            exit_code,
            samples,
        };

        export_json(&profile_result, &self.output_path)?;
        export_csv(&profile_result, &self.output_path.with_extension("csv"))?;

        debug!(
            "Profiling completed with {} samples collected",
            profile_result.samples.len()
        );
        Ok(profile_result)
    }

    /// Backward compatibility method that spawns a command and profiles it
    /// Use profile_process instead for more control over process launching
    pub fn profile_command(&mut self, command: &str) -> Result<ProfileResult> {
        info!("Profiling command: {command}");
        debug!("Will sample every {} seconds", self.sample_interval);

        // Spawning the command directly without CPU affinity
        // For CPU affinity control, use profile_process instead
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;

        self.profile_process(command, child)
    }
}

/// Collect a sample for a process and all its children
fn collect_process_sample(sys: &System, parent_pid: Pid, elapsed_seconds: u64) -> ProfileSample {
    let mut all_pids = Vec::new();
    get_all_related_pids(sys, parent_pid, &mut all_pids);

    let mut total_cpu = 0.0;
    let mut total_memory = 0;
    let mut total_virtual_memory = 0;
    let mut total_disk_read = 0;
    let mut total_disk_write = 0;

    for &pid in &all_pids {
        if let Some(process) = sys.process(pid) {
            total_cpu += process.cpu_usage();
            total_memory += process.memory();
            total_virtual_memory += process.virtual_memory();
            let disk_usage = process.disk_usage();
            total_disk_read += disk_usage.read_bytes;
            total_disk_write += disk_usage.written_bytes;
        }
    }

    ProfileSample {
        time: elapsed_seconds,
        cpu_usage: total_cpu,
        memory: total_memory,
        virtual_memory: total_virtual_memory,
        disk_read: total_disk_read,
        disk_write: total_disk_write,
    }
}

/// Recursively collect process tree PIDs
fn get_all_related_pids(sys: &System, parent_pid: Pid, result: &mut Vec<Pid>) {
    // Check if the parent_pid exists in the system before adding it
    if sys.process(parent_pid).is_some() && !result.contains(&parent_pid) {
        result.push(parent_pid);

        // Find all children recursively
        for process in sys.processes().values() {
            if let Some(parent) = process.parent() {
                if parent == parent_pid {
                    get_all_related_pids(sys, process.pid(), result);
                }
            }
        }
    } else if !result.contains(&parent_pid) {
        // If the parent isn't in the system but we're looking for it,
        // add it anyway so we can detect when it's gone
        result.push(parent_pid);
    }

    // If no processes were found and the parent was in the list,
    // it means all processes have likely terminated
    if result.len() == 1 && result[0] == parent_pid && sys.process(parent_pid).is_none() {
        debug!(
            "No processes found for PID {}, it may have terminated",
            parent_pid.as_u32()
        );
    }
}

/// Export profile results to JSON
fn export_json(result: &ProfileResult, path: &Path) -> Result<()> {
    let json_data = serde_json::to_string_pretty(result)?;
    std::fs::write(path, json_data)?;
    Ok(())
}

/// Export profile results to CSV
fn export_csv(result: &ProfileResult, path: &Path) -> Result<()> {
    let mut file = File::create(path)?;
    writeln!(file, "time,cpu,memory,virtual_memory,disk_read,disk_write")?;

    for sample in &result.samples {
        writeln!(
            file,
            "{},{},{},{},{},{}",
            sample.time,
            sample.cpu_usage,
            sample.memory,
            sample.virtual_memory,
            sample.disk_read,
            sample.disk_write
        )?;
    }

    Ok(())
}
