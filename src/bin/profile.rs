use std::fs::File;
use std::io::Write;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use sysinfo::{Pid, System};

fn get_child_pids(sys: &System, parent_pid: Pid) -> Vec<Pid> {
    let mut pids = vec![parent_pid];
    for (pid, process) in sys.processes() {
        if process.parent() == Some(parent_pid) {
            pids.extend(get_child_pids(sys, *pid));
        }
    }
    pids
}

fn profile(mut child: std::process::Child) -> Result<(), Box<dyn std::error::Error>> {
    let parent_pid = child.id();
    let mut sys = System::new_all();
    let start_time = Instant::now();

    let mut file = File::create("usage_data.csv")?;
    writeln!(file, "time,cpu,memory,virtual_memory,disk_read,disk_write")?;

    while child.try_wait()?.is_none() {
        sys.refresh_all();
        let pids = get_child_pids(&sys, Pid::from_u32(parent_pid));
        let mut total_cpu = 0.0;
        let mut total_memory = 0;
        let mut total_virtual_memory = 0;
        let mut total_disk_read = 0;
        let mut total_disk_write = 0;

        for &pid in &pids {
            if let Some(process) = sys.process(pid) {
                total_cpu += process.cpu_usage();
                total_memory += process.memory();
                total_virtual_memory += process.virtual_memory();
                let disk_usage = process.disk_usage();
                total_disk_read += disk_usage.total_read_bytes;
                total_disk_write += disk_usage.total_written_bytes;
            }
        }

        let elapsed = start_time.elapsed().as_secs();

        writeln!(
            file,
            "{},{},{},{},{},{}",
            elapsed,
            total_cpu,
            total_memory,
            total_virtual_memory,
            total_disk_read,
            total_disk_write
        )?;
        file.flush()?;

        std::thread::sleep(Duration::from_secs(5));
    }

    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cmd = "stress";
    let args = &[
        "--cpu",
        "10",
        "--io",
        "4",
        "--vm",
        "2",
        "--vm-bytes",
        "128M",
        "--timeout",
        "60s",
        "--hdd",
        "4",
    ];
    let child = Command::new(cmd).args(args).stdout(Stdio::null()).spawn()?;
    profile(child)?;
    Ok(())
}
