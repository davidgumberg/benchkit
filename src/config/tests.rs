#[cfg(test)]
mod tests {
    use crate::config::{
        benchmark::{
            is_valid_cpu_cores, merge_benchmark_options, BenchmarkGlobalConfig, BenchmarkOptions,
            SingleConfig,
        },
        traits::{Configuration, PathConfiguration},
    };
    use serde_json::Value;
    use std::collections::HashMap;
    use std::fs;
    use std::io::Write;
    use std::path::{Path, PathBuf};
    use tempfile::tempdir;

    // Helper function to assert paths are equal after canonicalization
    // This handles cases where paths may contain symlinks (e.g., /tmp -> /private/tmp on macOS)
    fn assert_canonical_path_eq(actual: &Path, expected: &Path) {
        let expected_canonical = expected.canonicalize()
            .unwrap_or_else(|e| panic!("Failed to canonicalize expected path {:?}: {}", expected, e));
        assert_eq!(actual, expected_canonical, 
            "Path mismatch: actual {:?} != expected {:?} (canonical: {:?})", 
            actual, expected, expected_canonical);
    }

    #[test]
    fn test_is_valid_cpu_cores() {
        // Valid formats
        assert!(is_valid_cpu_cores("0"));
        assert!(is_valid_cpu_cores("0,1,2"));
        assert!(is_valid_cpu_cores("0-3"));
        assert!(is_valid_cpu_cores("0-3,5,7-9"));

        // Invalid formats
        assert!(!is_valid_cpu_cores(""));
        assert!(!is_valid_cpu_cores("a"));
        assert!(!is_valid_cpu_cores("0-"));
        assert!(!is_valid_cpu_cores("-3"));
        assert!(!is_valid_cpu_cores("0-3-5"));
        assert!(!is_valid_cpu_cores("0,a,2"));
    }

    #[test]
    fn test_benchmark_options_defaults() {
        let opts = BenchmarkOptions::new();

        assert_eq!(opts.warmup, 0);
        assert_eq!(opts.runs, 1);
        assert!(!opts.capture_output);
        assert!(opts.command.is_none());
        assert!(opts.parameter_lists.is_none());
        assert!(opts.profile.is_none());
        assert!(opts.profile_interval.is_none());
    }

    #[test]
    fn test_benchmark_options_validation() {
        // Valid options
        let valid_opts = BenchmarkOptions::new();
        assert!(valid_opts.validate().is_ok());

        // Invalid profile interval
        let mut invalid_profile_interval = BenchmarkOptions::new();
        invalid_profile_interval.profile = Some(true);
        invalid_profile_interval.profile_interval = Some(0);
        assert!(invalid_profile_interval.validate().is_err());

        // Missing command for execution
        let no_command = BenchmarkOptions::new();
        assert!(no_command.validate_for_execution().is_err());

        // Valid for execution
        let mut with_command = BenchmarkOptions::new();
        with_command.command = Some("test command".to_string());
        assert!(with_command.validate_for_execution().is_ok());
    }

    #[test]
    fn test_merge_benchmark_options() {
        // Base options
        let base_opts = BenchmarkOptions {
            warmup: 1,
            runs: 2,
            capture_output: false,
            command: Some("base command".to_string()),
            parameter_lists: None,
            profile: Some(false),
            profile_interval: Some(5),
            stop_on_log_pattern: None,
        };

        // Override map
        let mut override_map = HashMap::new();
        override_map.insert("warmup".to_string(), Value::from(3));
        override_map.insert("runs".to_string(), Value::from(4));
        override_map.insert("capture_output".to_string(), Value::from(true));
        override_map.insert("command".to_string(), Value::from("override command"));
        override_map.insert("profile".to_string(), Value::from(true));

        // Merge options
        let merged = merge_benchmark_options(&Some(base_opts.clone()), &override_map).unwrap();

        // Check merged values
        assert_eq!(merged.warmup, 3);
        assert_eq!(merged.runs, 4);
        assert!(merged.capture_output);
        assert_eq!(merged.command, Some("override command".to_string()));
        assert_eq!(merged.profile, Some(true));
        assert_eq!(merged.profile_interval, Some(5)); // Unchanged
    }

    #[test]
    fn test_single_config_validation() {
        // Valid config
        let valid = SingleConfig {
            name: "test".to_string(),
            env: None,
            network: "main".to_string(),
            connect: None,
            scripts: None,
            benchmark: HashMap::new(),
        };
        assert!(valid.validate().is_ok());

        // Empty name
        let invalid_name = SingleConfig {
            name: "".to_string(),
            env: None,
            network: "main".to_string(),
            connect: None,
            scripts: None,
            benchmark: HashMap::new(),
        };
        assert!(invalid_name.validate().is_err());

        // Invalid network
        let invalid_network = SingleConfig {
            name: "test".to_string(),
            env: None,
            network: "invalid".to_string(),
            connect: None,
            scripts: None,
            benchmark: HashMap::new(),
        };
        assert!(invalid_network.validate().is_err());
    }

    #[test]
    fn test_path_expansion() {
        let tempdir = tempdir().unwrap();
        let config_dir = tempdir.path();

        // Create a BenchmarkGlobalConfig with relative paths
        let config = BenchmarkGlobalConfig {
            benchmark: None,
            scripts: None,
            benchmark_cores: None,
            runner_cores: None,
            cmake_build_args: None,
            source: PathBuf::from("./source"),
            scratch: PathBuf::from("./scratch"),
            tmp_data_dir: PathBuf::from("./tmp"),
            commits: vec!["commit1".to_string()],
        };

        // Expand paths
        let expanded = config.with_expanded_paths(config_dir).unwrap();

        // Check that paths are now absolute
        assert!(expanded.source.is_absolute());
        assert!(expanded.scratch.is_absolute());
        assert!(expanded.tmp_data_dir.is_absolute());

        // Check that paths point to the correct location
        assert_canonical_path_eq(&expanded.source, &config_dir.join("source"));
        assert_canonical_path_eq(&expanded.scratch, &config_dir.join("scratch"));
        assert_canonical_path_eq(&expanded.tmp_data_dir, &config_dir.join("tmp"));
    }

    #[test]
    fn test_load_app_config() {
        let tempdir = tempdir().unwrap();
        let config_path = tempdir.path().join("config.yml");

        // Create a sample config file
        let config_content = r#"
        bin_dir: ./bin
        home_dir: ./home
        patch_dir: ./patches
        snapshot_dir: ./snapshots
        "#;

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        // Create the directories before loading (required for canonicalization)
        fs::create_dir_all(tempdir.path().join("bin")).unwrap();
        fs::create_dir_all(tempdir.path().join("home")).unwrap();
        fs::create_dir_all(tempdir.path().join("patches")).unwrap();
        fs::create_dir_all(tempdir.path().join("snapshots")).unwrap();

        // Load the config
        let config = crate::config::app::load_app_config(&config_path).unwrap();

        // Check values using our helper
        assert_canonical_path_eq(&config.bin_dir, &tempdir.path().join("bin"));
        assert_canonical_path_eq(&config.home_dir, &tempdir.path().join("home"));
        assert_canonical_path_eq(&config.patch_dir, &tempdir.path().join("patches"));
        assert_canonical_path_eq(&config.snapshot_dir, &tempdir.path().join("snapshots"));
        assert_eq!(config.path, config_path);

        // Check trait implementations
        assert_eq!(config.config_type(), "application");
        assert_eq!(config.config_path(), &config_path);
    }

    #[test]
    fn test_load_bench_config() {
        let tempdir = tempdir().unwrap();
        let config_path = tempdir.path().join("benchmark.yml");

        // Create the directories
        fs::create_dir_all(tempdir.path().join("source")).unwrap();
        fs::create_dir_all(tempdir.path().join("scratch")).unwrap();
        fs::create_dir_all(tempdir.path().join("tmp")).unwrap();

        // Create a sample benchmark config file
        let config_content = r#"
        global:
          source: ./source
          scratch: ./scratch
          tmp_data_dir: ./tmp
          commits:
            - abcdef123456
        benchmarks:
          - name: test_bench
            network: main
            benchmark:
              command: "echo test"
              runs: 3
        "#;

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        // Load the config with a run ID
        let config = crate::config::benchmark::load_bench_config(&config_path, 12345).unwrap();

        // Check values
        assert_eq!(config.run_id, 12345);
        assert_eq!(config.path, config_path);
        // Check paths using our helper
        assert_canonical_path_eq(&config.global.source, &tempdir.path().join("source"));
        assert_canonical_path_eq(&config.global.scratch, &tempdir.path().join("scratch"));
        assert_canonical_path_eq(&config.global.tmp_data_dir, &tempdir.path().join("tmp"));
        assert_eq!(config.global.commits, vec!["abcdef123456"]);

        assert_eq!(config.benchmarks.len(), 1);
        assert_eq!(config.benchmarks[0].name, "test_bench");
        assert_eq!(config.benchmarks[0].network, "main");

        // Test configuration trait
        assert_eq!(config.config_type(), "benchmark");
        assert_eq!(config.config_path(), &config_path);
    }

    #[test]
    fn test_global_config() {
        let tempdir = tempdir().unwrap();

        // Create app config file
        let app_config_path = tempdir.path().join("config.yml");
        let app_content = r#"
        bin_dir: ./bin
        home_dir: ./home
        patch_dir: ./patches
        snapshot_dir: ./snapshots
        "#;

        let mut app_file = fs::File::create(&app_config_path).unwrap();
        app_file.write_all(app_content.as_bytes()).unwrap();

        // Create benchmark config file
        let bench_config_path = tempdir.path().join("benchmark.yml");

        // Create the directories
        fs::create_dir_all(tempdir.path().join("source")).unwrap();
        fs::create_dir_all(tempdir.path().join("scratch")).unwrap();
        fs::create_dir_all(tempdir.path().join("tmp")).unwrap();
        fs::create_dir_all(tempdir.path().join("bin")).unwrap();
        fs::create_dir_all(tempdir.path().join("patches")).unwrap();
        fs::create_dir_all(tempdir.path().join("snapshots")).unwrap();

        let bench_content = r#"
        global:
          source: ./source
          scratch: ./scratch
          tmp_data_dir: ./tmp
          commits:
            - abcdef123456
        benchmarks:
          - name: test_bench
            network: main
            benchmark:
              command: "echo test"
        "#;

        let mut bench_file = fs::File::create(&bench_config_path).unwrap();
        bench_file.write_all(bench_content.as_bytes()).unwrap();

        // Load configs
        let app_config = crate::config::app::load_app_config(&app_config_path).unwrap();
        let bench_config =
            crate::config::benchmark::load_bench_config(&bench_config_path, 12345).unwrap();

        // Create global config
        let global_config = crate::config::GlobalConfig {
            app: app_config,
            bench: bench_config,
        };

        // Test configuration trait
        assert_eq!(global_config.config_type(), "global");
        assert_eq!(global_config.config_path(), &bench_config_path);
        assert!(global_config.validate().is_ok());
    }

    #[test]
    fn test_config_adapter() {
        let tempdir = tempdir().unwrap();
        let config_path = tempdir.path().join("benchmark.yml");

        // Create the directories
        fs::create_dir_all(tempdir.path().join("source")).unwrap();
        fs::create_dir_all(tempdir.path().join("scratch")).unwrap();
        fs::create_dir_all(tempdir.path().join("tmp")).unwrap();

        // Create a sample benchmark config file with global and specific options
        let config_content = r#"
        global:
          source: ./source
          scratch: ./scratch
          tmp_data_dir: ./tmp
          commits:
            - abcdef123456
          benchmark:
            warmup: 2
            runs: 5
            capture_output: true
        benchmarks:
          - name: test_bench1
            network: main
            benchmark:
              command: "echo test1"
              runs: 10
          - name: test_bench2
            network: test
            benchmark:
              command: "echo test2"
              warmup: 3
              profile: true
              profile_interval: 5
        "#;

        let mut file = fs::File::create(&config_path).unwrap();
        file.write_all(config_content.as_bytes()).unwrap();

        // Load the config
        let config = crate::config::benchmark::load_bench_config(&config_path, 12345).unwrap();

        // Test the first benchmark merging
        let options1 =
            crate::config::adapter::ConfigAdapter::get_merged_options(&config, 0).unwrap();

        assert_eq!(options1.warmup, 2); // From global
        assert_eq!(options1.runs, 10); // Overridden in benchmark1
        assert!(options1.capture_output); // From global
        assert_eq!(options1.command, Some("echo test1".to_string())); // From benchmark1
        assert!(options1.profile.is_none()); // Not specified

        // Test the second benchmark merging
        let options2 =
            crate::config::adapter::ConfigAdapter::get_merged_options(&config, 1).unwrap();

        assert_eq!(options2.warmup, 3); // Overridden in benchmark2
        assert_eq!(options2.runs, 5); // From global
        assert!(options2.capture_output); // From global
        assert_eq!(options2.command, Some("echo test2".to_string())); // From benchmark2
        assert_eq!(options2.profile, Some(true)); // From benchmark2
        assert_eq!(options2.profile_interval, Some(5)); // From benchmark2
    }
}
