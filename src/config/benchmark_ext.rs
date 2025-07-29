use anyhow::Result;
use serde_json::Value;
use std::collections::HashMap;

use crate::config::benchmark::BenchmarkOptions;
use crate::config::merge::{Merge, MergeFromMap};

/// Implementation of Merge trait for BenchmarkOptions
impl Merge for BenchmarkOptions {
    fn merge(&self, other: &Self) -> Result<Self> {
        Ok(Self {
            warmup: other.warmup,
            runs: other.runs,
            capture_output: other.capture_output,
            command: other.command.clone().or_else(|| self.command.clone()),
            parameter_lists: other
                .parameter_lists
                .clone()
                .or_else(|| self.parameter_lists.clone()),
            profile: other.profile.or(self.profile),
            profile_interval: other.profile_interval.or(self.profile_interval),
            stop_on_log_pattern: other
                .stop_on_log_pattern
                .clone()
                .or_else(|| self.stop_on_log_pattern.clone()),
        })
    }
}

/// Implementation of MergeFromMap trait for BenchmarkOptions
impl MergeFromMap<String, Value> for BenchmarkOptions {
    fn merge_from_map(&self, map: &HashMap<String, Value>) -> Result<Self> {
        // Start with a clone of self as the base
        let mut result = self.clone();

        // Apply overrides from the map
        if let Some(warmup) = map.get("warmup").and_then(|v| v.as_u64()) {
            result.warmup = warmup as usize;
        }

        if let Some(runs) = map.get("runs").and_then(|v| v.as_u64()) {
            result.runs = runs as usize;
        }

        if let Some(capture_output) = map.get("capture_output").and_then(|v| v.as_bool()) {
            result.capture_output = capture_output;
        }

        if let Some(command) = map.get("command").and_then(|v| v.as_str()) {
            result.command = Some(command.to_string());
        }

        if let Some(parameter_lists) = map.get("parameter_lists").and_then(|v| v.as_array()) {
            result.parameter_lists = Some(parameter_lists.clone());
        }

        if let Some(profile) = map.get("profile").and_then(|v| v.as_bool()) {
            result.profile = Some(profile);
        }

        if let Some(profile_interval) = map.get("profile_interval").and_then(|v| v.as_u64()) {
            result.profile_interval = Some(profile_interval);
        }

        if let Some(stop_on_log_pattern) = map.get("stop_on_log_pattern").and_then(|v| v.as_str()) {
            result.stop_on_log_pattern = Some(stop_on_log_pattern.to_string());
        }

        Ok(result)
    }
}
