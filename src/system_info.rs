use anyhow::Result;
use log::info;
use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use sysinfo::System;

#[rustfmt::skip]
pub fn dump_sys_info(file: &PathBuf) -> Result<()> {
    info!("Writing system info to {file:?}");
    let mut file = File::create(file)?;
    let mut sys = System::new_all();
    sys.refresh_all();

    {
    writeln!(file, "{:<25}{}", "System name:", System::name().unwrap_or_else(|| "<unknown>".to_owned()))?;
    writeln!(file, "{:<25}{}", "System kernel version:", System::kernel_version().unwrap_or_else(|| "<unknown>".to_owned()))?;
    writeln!(file, "{:<25}{}", "System OS version:", System::long_os_version().unwrap_or_else(|| "<unknown>".to_owned()))?;
    writeln!(file, "{:<25}{}", "Distribution ID:", System::distribution_id())?;
    }
    // Cpu info
    writeln!(file, "{:<25}{}", "CPU Arch:", System::cpu_arch())?;
    let processors = sys.cpus();
    if !processors.is_empty() {
        let processor = &processors[0];
        let cpu_brand = processor.brand();
        let cpu_count = processors.len();
        let cpu_freq = processor.frequency();
        let cpu_name = processor.name();
    writeln!(file, "{:<25}{} {} ({}) @ {:.2} GHz",
        "CPU:",
            cpu_name,
            cpu_brand,
            cpu_count,
            cpu_freq as f64 / 1000.0)?;
    } else {
        writeln!(file, "CPU: Unknown")?;
    }

    // RAM and swap:
    writeln!(file, "{:<25}{} bytes", "Total memory:", sys.total_memory())?;
    writeln!(file, "{:<25}{} bytes", "Used memory:", sys.used_memory())?;
    writeln!(file, "{:<25}{} bytes", "Total swap:", sys.total_swap())?;
    writeln!(file, "{:<25}{} bytes", "Used swap:", sys.used_swap())?;

    // Uptime
    let uptime = System::uptime();
    writeln!(file, "{:<25}{}", "Uptime (seconds):", uptime)?;
    writeln!(file, "{:<25}{}", "Uptime (days):", uptime / 86400)?;
    Ok(())
}
