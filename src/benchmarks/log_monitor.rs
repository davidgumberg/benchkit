use anyhow::{Context, Result};
use log::{debug, info, trace};
use regex::Regex;
use std::io::{BufRead, BufReader};
use std::process::Child;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Monitors process output for a specific regex pattern
pub struct LogMonitor {
    /// The pattern to search for
    pattern: String,
    /// Flag indicating if the pattern was matched
    matched: Arc<AtomicBool>,
    /// Thread handles for stdout and stderr readers
    reader_threads: Vec<thread::JoinHandle<Result<()>>>,
}

impl LogMonitor {
    /// Start monitoring a child process for a specific regex pattern
    pub fn start_monitoring(child: &mut Child, pattern: String) -> Result<Self> {
        debug!("Starting log monitor for pattern: {pattern}");

        // Create and compile the regex pattern
        let regex = Regex::new(&pattern).context("Failed to compile regex pattern")?;
        let regex = Arc::new(regex);

        let matched = Arc::new(AtomicBool::new(false));
        let mut reader_threads = Vec::new();

        // Take stdout if available
        if let Some(stdout) = child.stdout.take() {
            let thread_regex = Arc::clone(&regex);
            let thread_matched = Arc::clone(&matched);

            let handle = thread::spawn(move || {
                monitor_stream_with_regex(stdout, thread_regex, thread_matched, "stdout")
            });
            reader_threads.push(handle);
        }

        // Take stderr if available
        if let Some(stderr) = child.stderr.take() {
            let thread_regex = Arc::clone(&regex);
            let thread_matched = Arc::clone(&matched);

            let handle = thread::spawn(move || {
                monitor_stream_with_regex(stderr, thread_regex, thread_matched, "stderr")
            });
            reader_threads.push(handle);
        }

        Ok(LogMonitor {
            pattern,
            matched,
            reader_threads,
        })
    }

    /// Check if the pattern has been matched
    pub fn is_matched(&self) -> bool {
        self.matched.load(Ordering::SeqCst)
    }

    /// Wait for either the pattern to be matched or the process to exit
    /// Returns true if pattern was matched, false if process exited
    pub fn wait_for_match_or_exit(
        &mut self,
        child: &mut Child,
        check_interval: Duration,
    ) -> Result<bool> {
        debug!("Waiting for pattern match or process exit");
        let mut check_count = 0;

        loop {
            check_count += 1;

            // Check if process has exited
            match child.try_wait()? {
                Some(status) => {
                    debug!(
                        "Process exited with status {status} before pattern match (after {check_count} checks)"
                    );
                    self.cleanup_threads();
                    return Ok(false);
                }
                None => {
                    // Process still running, check for match
                    if self.is_matched() {
                        info!(
                            "Pattern '{}' matched in process output after {} checks",
                            self.pattern, check_count
                        );
                        return Ok(true);
                    }

                    // Log periodic status
                    if check_count % 100 == 0 {
                        debug!("Still waiting for pattern match after {check_count} checks, process still running");
                    }

                    // Sleep before next check
                    thread::sleep(check_interval);
                }
            }
        }
    }

    /// Clean up reader threads
    fn cleanup_threads(&mut self) {
        for handle in self.reader_threads.drain(..) {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => debug!("Reader thread error: {e}"),
                Err(_) => debug!("Reader thread panicked"),
            }
        }
    }
}

impl Drop for LogMonitor {
    fn drop(&mut self) {
        self.cleanup_threads();
    }
}

/// Monitor a single stream for the pattern using regex
fn monitor_stream_with_regex<R: std::io::Read + Send + 'static>(
    stream: R,
    regex: Arc<Regex>,
    matched: Arc<AtomicBool>,
    stream_name: &str,
) -> Result<()> {
    let reader = BufReader::new(stream);

    debug!("Starting to monitor {stream_name} stream");
    let mut line_count = 0;

    for line_result in reader.lines() {
        match line_result {
            Ok(line) => {
                line_count += 1;

                // Log every line to trace what we're receiving
                trace!("{stream_name}:{line_count} - {line}");

                // Check if line matches the pattern using regex
                if regex.is_match(&line) {
                    info!("REGEX MATCH: line={line}");
                    info!("Pattern matched in {stream_name} line: {line}");
                    matched.store(true, Ordering::SeqCst);
                    trace!("Set matched flag to true in {stream_name} thread");
                    break;
                }
            }
            Err(e) => {
                // EOF or other error, stop reading
                debug!("Error reading from {stream_name} after {line_count} lines: {e}");
                break;
            }
        }
    }

    debug!("Finished monitoring {stream_name} stream after {line_count} lines");
    Ok(())
}

/// Builder for LogMonitor with configurable options
pub struct LogMonitorBuilder {
    check_interval: Duration,
}

impl Default for LogMonitorBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl LogMonitorBuilder {
    /// Create a new LogMonitorBuilder
    pub fn new() -> Self {
        Self {
            check_interval: Duration::from_millis(100),
        }
    }

    /// Set the interval for checking match status
    pub fn check_interval(mut self, interval: Duration) -> Self {
        self.check_interval = interval;
        self
    }

    /// Build and start monitoring
    pub fn start(self, child: &mut Child, pattern: String) -> Result<LogMonitor> {
        LogMonitor::start_monitoring(child, pattern)
    }
}
