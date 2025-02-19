use anyhow::Result;
use clap::Subcommand;
use std::{fs, path::Path, process::Command};

#[derive(Subcommand, Debug)]
pub enum SystemCommands {
    Check,
    Tune,
    Reset,
}

pub struct SystemChecker {
    cpus: Vec<usize>,
}

impl SystemChecker {
    pub fn new() -> Result<Self> {
        let mut cpu_count: Vec<usize> = fs::read_dir("/sys/devices/system/cpu")?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().into_string().ok()?;
                if name.starts_with("cpu") && name[3..].parse::<usize>().is_ok() {
                    Some(name[3..].parse::<usize>().unwrap())
                } else {
                    None
                }
            })
            .collect();
        cpu_count.sort();

        Ok(Self { cpus: cpu_count })
    }

    fn check_aslr() -> Result<bool> {
        let aslr = fs::read_to_string("/proc/sys/kernel/randomize_va_space")?;
        Ok(aslr.trim() == "2")
    }

    fn check_cpu_isolation() -> Result<(bool, bool)> {
        let cmdline = fs::read_to_string("/proc/cmdline")?;
        let has_isolcpus = cmdline.contains("isolcpus=");
        let has_rcu_nocbs = cmdline.contains("rcu_nocbs=");
        Ok((has_isolcpus, has_rcu_nocbs))
    }

    fn check_intel_pstate() -> Result<bool> {
        Ok(Path::new("/sys/devices/system/cpu/intel_pstate").exists())
    }

    fn get_scaling_governor(&self, cpu: usize) -> Result<String> {
        let path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor",
            cpu
        );
        Ok(fs::read_to_string(path)?.trim().to_string())
    }

    fn set_scaling_governor(&self, cpu: usize, governor: &str) -> Result<()> {
        let path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor",
            cpu
        );
        fs::write(path, governor)?;
        Ok(())
    }

    fn get_cpu_freq(&self, cpu: usize) -> Result<(u64, u64)> {
        let min_path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_min_freq",
            cpu
        );
        let max_path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_max_freq",
            cpu
        );

        let min = fs::read_to_string(&min_path)?.trim().parse()?;
        let max = fs::read_to_string(&max_path)?.trim().parse()?;

        Ok((min, max))
    }

    fn set_cpu_min_freq(&self, cpu: usize, freq: u64) -> Result<()> {
        let path = format!(
            "/sys/devices/system/cpu/cpu{}/cpufreq/scaling_min_freq",
            cpu
        );
        fs::write(path, freq.to_string())?;
        Ok(())
    }

    fn check_irqbalance() -> Result<bool> {
        let status = Command::new("systemctl")
            .args(["is-active", "irqbalance"])
            .output()?;
        Ok(status.status.success())
    }

    fn set_irqbalance(&self, enable: bool) -> Result<()> {
        let action = if enable { "start" } else { "stop" };
        Command::new("systemctl")
            .args([action, "irqbalance"])
            .status()?;
        Ok(())
    }

    fn get_perf_sample_rate() -> Result<u32> {
        let rate = fs::read_to_string("/proc/sys/kernel/perf_event_max_sample_rate")?;
        Ok(rate.trim().parse()?)
    }

    #[allow(dead_code)]
    fn set_perf_sample_rate(rate: u32) -> Result<()> {
        fs::write(
            "/proc/sys/kernel/perf_event_max_sample_rate",
            rate.to_string(),
        )?;
        Ok(())
    }

    fn check_power_supply() -> Result<bool> {
        let ac_online = Path::new("/sys/class/power_supply/AC/online");
        if ac_online.exists() {
            let status = fs::read_to_string(ac_online)?.trim().parse::<u8>()?;
            Ok(status == 1)
        } else {
            // Assume desktop/server if no battery info
            Ok(true)
        }
    }

    fn check_turbo_boost(&self) -> Result<bool> {
        if Self::check_intel_pstate()? {
            let no_turbo = fs::read_to_string("/sys/devices/system/cpu/intel_pstate/no_turbo")?;
            Ok(no_turbo.trim() == "0")
        } else {
            // TODO: Implement MSR reading for non-pstate systems
            Ok(false)
        }
    }

    fn set_turbo_boost(&self, enable: bool) -> Result<()> {
        if Self::check_intel_pstate()? {
            fs::write(
                "/sys/devices/system/cpu/intel_pstate/no_turbo",
                if enable { "0" } else { "1" },
            )?;
        }
        Ok(())
    }

    pub fn run_checks(&self) -> Result<()> {
        println!("System Performance Checks:");

        println!("\nKernel Settings:");
        match Self::check_aslr()? {
            true => println!("✓ ASLR: Full randomization enabled (want: enabled)"),
            false => println!("✗ ASLR: Full randomization not enabled (want: enabled)"),
        }

        let (has_isolcpus, has_rcu_nocbs) = Self::check_cpu_isolation()?;
        println!(
            "{} CPU Isolation: {} (want: set)",
            if has_isolcpus { "✓" } else { "✗" },
            if has_isolcpus { "set" } else { "not set" }
        );
        println!(
            "{} RCU: {} (want: set)",
            if has_rcu_nocbs { "✓" } else { "✗" },
            if has_rcu_nocbs { "set" } else { "not set" }
        );

        println!("\nCPU Settings:");
        for cpu in &self.cpus {
            let governor = self.get_scaling_governor(*cpu)?;
            let (min_freq, max_freq) = self.get_cpu_freq(*cpu)?;
            let gov_status = if governor == "performance" {
                "✓"
            } else {
                "✗"
            };
            println!(
                "{} CPU {:2}: Governor: {} (want: performance), Freq: {}-{} KHz",
                gov_status, cpu, governor, min_freq, max_freq
            );
        }

        println!("\nSystem Settings:");
        let irq_status = if !Self::check_irqbalance()? {
            "✓"
        } else {
            "✗"
        };
        println!(
            "{} IRQ Balancing: {} (want: inactive)",
            irq_status,
            if Self::check_irqbalance()? {
                "active"
            } else {
                "inactive"
            }
        );

        let perf_rate = Self::get_perf_sample_rate()?;
        let perf_status = if perf_rate == 1 { "✓" } else { "✗" };
        println!("{} Perf sample rate: {} (want: 1)", perf_status, perf_rate);

        let power_status = if Self::check_power_supply()? {
            "✓"
        } else {
            "✗"
        };
        println!(
            "{} Power Supply: {} (want: AC power)",
            power_status,
            if Self::check_power_supply()? {
                "AC power"
            } else {
                "battery"
            }
        );

        let turbo_status = if self.check_turbo_boost()? {
            "✓"
        } else {
            "✗"
        };
        println!(
            "{} Turbo Boost: {} (want: enabled)",
            turbo_status,
            if self.check_turbo_boost()? {
                "enabled"
            } else {
                "disabled"
            }
        );

        Ok(())
    }

    pub fn tune(&self) -> Result<()> {
        println!("Tuning system for benchmarking...");

        // Set CPU governor to performance
        for cpu in &self.cpus {
            self.set_scaling_governor(*cpu, "performance")?;

            // Set min frequency to max
            let (_, max_freq) = self.get_cpu_freq(*cpu)?;
            self.set_cpu_min_freq(*cpu, max_freq)?;
        }

        // Stop IRQ balancing
        self.set_irqbalance(false)?;

        // Set perf sample rate to minimum
        // We never want to alter this
        // Self::set_perf_sample_rate(1)?;

        // Enable Turbo Boost
        self.set_turbo_boost(true)?;

        println!("System tuned for benchmarking");
        Ok(())
    }

    pub fn reset(&self) -> Result<()> {
        println!("Resetting system to default settings...");

        // Reset CPU governor to powersave
        for cpu in &self.cpus {
            self.set_scaling_governor(*cpu, "powersave")?;

            // Reset min frequency to minimum
            let (min_freq, _) = self.get_cpu_freq(*cpu)?;
            self.set_cpu_min_freq(*cpu, min_freq)?;
        }

        // Start IRQ balancing
        self.set_irqbalance(true)?;

        // Reset perf sample rate
        // Self::set_perf_sample_rate(100_000)?;

        // Reset Turbo Boost to default (enabled)
        self.set_turbo_boost(true)?;

        println!("System reset to default settings");
        Ok(())
    }
}
