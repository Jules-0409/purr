// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;
use std::process::Stdio;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::task::JoinHandle;

const RESTART_DELAY: Duration = Duration::from_secs(3);
static RUNNING_PID: AtomicU32 = AtomicU32::new(0);

fn set_pid(slot: &AtomicU32, value: u32) {
    slot.store(value, Ordering::SeqCst);
    RUNNING_PID.store(value, Ordering::SeqCst);
}

#[allow(dead_code)]
pub fn running_pid() -> u32 {
    RUNNING_PID.load(Ordering::SeqCst)
}

pub fn terminate_running() {
    let pid = RUNNING_PID.swap(0, Ordering::SeqCst);
    if pid != 0 {
        request_stop(pid);
    }
}

pub struct Core {
    shutdown_tx: watch::Sender<bool>,
    task: JoinHandle<()>,
    pid: Arc<AtomicU32>,
}

impl Core {
    pub fn start(binary: PathBuf, config_dir: PathBuf) -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let pid = Arc::new(AtomicU32::new(0));
        let task_pid = Arc::clone(&pid);
        let task = tokio::spawn(async move {
            supervise(binary, config_dir, shutdown_rx, task_pid).await;
        });
        Self {
            shutdown_tx,
            task,
            pid,
        }
    }

    pub fn pid(&self) -> u32 {
        self.pid.load(Ordering::SeqCst)
    }

    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let _ = self.task.await;
    }
}

async fn supervise(
    binary: PathBuf,
    config_dir: PathBuf,
    mut shutdown: watch::Receiver<bool>,
    pid: Arc<AtomicU32>,
) {
    #[cfg(windows)]
    let job = windows_job::Job::new()
        .map_err(|e| eprintln!("purr: core job setup: {e}"))
        .ok();
    #[cfg(unix)]
    {
        let dir = config_dir.clone();
        let core = binary.clone();
        if let Ok(stopped) =
            tokio::task::spawn_blocking(move || stop_stale_cores(&core, &dir)).await
        {
            if !stopped.is_empty() {
                eprintln!("purr: stopped leftover mihomo {stopped:?}");
            }
        }
    }
    loop {
        if *shutdown.borrow() {
            break;
        }
        let mut child = match spawn_core(&binary, &config_dir).await {
            Ok(child) => child,
            Err(err) => {
                eprintln!("purr: failed to start mihomo: {err}");
                if wait_or_shutdown(&mut shutdown, RESTART_DELAY).await {
                    break;
                }
                continue;
            }
        };
        let child_pid = child.id().unwrap_or(0);
        #[cfg(windows)]
        if let Some(job) = &job {
            if let Err(e) = job.assign(child_pid) {
                eprintln!("purr: core job assignment: {e}");
            }
        }
        set_pid(&pid, child_pid);
        eprintln!("purr: mihomo pid {child_pid}");

        tokio::select! {
            status = child.wait() => {
                set_pid(&pid, 0);
                if *shutdown.borrow() {
                    eprintln!("purr: mihomo stopped");
                    break;
                }
                eprintln!("purr: mihomo exited ({status:?}), restarting in 3s");
                if wait_or_shutdown(&mut shutdown, RESTART_DELAY).await {
                    break;
                }
            }
            _ = shutdown.changed() => {
                stop_child(&mut child).await;
                set_pid(&pid, 0);
                eprintln!("purr: mihomo stopped");
                break;
            }
        }
    }
}

/// A core whose Purr died without shutting it down (killed, crashed, or replaced by an update while
/// running) keeps the controller, mixed port and TUN. The next core then fails to bind, keeps
/// running without a controller, and the UI reads statistics from an instance nobody manages.
/// Only orphans (parent pid 1) that run our core binary on our config directory are stopped.
/// The setuid TUN copy keeps the user's real uid, so a plain SIGTERM is permitted.
#[cfg(unix)]
pub fn stop_stale_cores(binary: &std::path::Path, config_dir: &std::path::Path) -> Vec<u32> {
    let Ok(out) = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,command="])
        .output()
    else {
        return Vec::new();
    };
    let pids = stale_core_pids(&String::from_utf8_lossy(&out.stdout), binary, config_dir);
    for pid in &pids {
        request_stop(*pid);
    }
    let alive = |pid: u32| unsafe { libc::kill(pid as i32, 0) } == 0;
    for _ in 0..30 {
        if !pids.iter().any(|p| alive(*p)) {
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    for pid in &pids {
        if alive(*pid) {
            unsafe {
                libc::kill(*pid as i32, libc::SIGKILL);
            }
        }
    }
    pids
}

#[cfg(unix)]
fn stale_core_pids(ps: &str, binary: &std::path::Path, config_dir: &std::path::Path) -> Vec<u32> {
    let name = binary.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let args = format!(" -d {} -f ", config_dir.display());
    ps.lines()
        .filter_map(|line| {
            let line = line.trim_start();
            let (pid, rest) = line.split_once(char::is_whitespace)?;
            let (ppid, command) = rest.trim_start().split_once(char::is_whitespace)?;
            let (exe, _) = command.split_once(&args)?;
            let ours = !name.is_empty()
                && std::path::Path::new(exe)
                    .file_name()
                    .and_then(|n| n.to_str())
                    == Some(name);
            (ours && ppid == "1").then(|| pid.parse().ok()).flatten()
        })
        .collect()
}

async fn spawn_core(binary: &PathBuf, config_dir: &PathBuf) -> Result<Child, String> {
    let log_path = config_dir.join("core.log");
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|err| format!("open {}: {err}", log_path.display()))?;
    let err_log = log
        .try_clone()
        .map_err(|err| format!("clone core log: {err}"))?;
    let mut command = Command::new(binary);
    command
        .arg("-d")
        .arg(config_dir)
        .arg("-f")
        .arg(config_dir.join("config.yaml"))
        .env("GOMEMLIMIT", "32MiB")
        .env("GOGC", "30")
        .env("GOMAXPROCS", "2")
        .env("GODEBUG", "madvdontneed=1")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(err_log))
        .kill_on_drop(true);
    #[cfg(unix)]
    {
        // Drop the controlling terminal so a dead shell cannot deliver SIGHUP
        // into the kernel before we have restarted it.
        command.process_group(0);
    }
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    command
        .spawn()
        .map_err(|err| format!("spawn {}: {err}", binary.display()))
}

async fn stop_child(child: &mut Child) {
    #[cfg(unix)]
    if let Some(pid) = child.id() {
        request_stop(pid);
        if tokio::time::timeout(Duration::from_secs(2), child.wait())
            .await
            .is_ok()
        {
            return;
        }
    }
    let _ = child.kill().await;
    let _ = tokio::time::timeout(Duration::from_secs(1), child.wait()).await;
}

fn request_stop(pid: u32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    #[cfg(windows)]
    unsafe {
        use std::ffi::c_void;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
            fn TerminateProcess(process: *mut c_void, exit_code: u32) -> i32;
            fn CloseHandle(handle: *mut c_void) -> i32;
        }
        let handle = OpenProcess(0x0001, 0, pid); // PROCESS_TERMINATE
        if !handle.is_null() {
            let _ = TerminateProcess(handle, 0);
            let _ = CloseHandle(handle);
        }
    }
}

#[cfg(windows)]
mod windows_job {
    use std::ffi::c_void;
    #[repr(C)]
    #[derive(Default)]
    struct Basic {
        process_time: i64,
        job_time: i64,
        flags: u32,
        minimum: usize,
        maximum: usize,
        active: u32,
        affinity: usize,
        priority: u32,
        scheduling: u32,
    }
    #[repr(C)]
    #[derive(Default)]
    struct Extended {
        basic: Basic,
        io: [u64; 6],
        process_memory: usize,
        job_memory: usize,
        peak_process: usize,
        peak_job: usize,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateJobObjectW(attributes: *mut c_void, name: *const u16) -> *mut c_void;
        fn SetInformationJobObject(
            job: *mut c_void,
            class: i32,
            info: *const Extended,
            size: u32,
        ) -> i32;
        fn AssignProcessToJobObject(job: *mut c_void, process: *mut c_void) -> i32;
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
    }
    pub struct Job(usize);
    impl Job {
        pub fn new() -> Result<Self, std::io::Error> {
            unsafe {
                let handle = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
                if handle.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                let job = Self(handle as usize);
                let mut info = Extended::default();
                info.basic.flags = 0x2000; // Kill children when the last job handle closes.
                if SetInformationJobObject(handle, 9, &info, std::mem::size_of::<Extended>() as u32)
                    == 0
                {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(job)
            }
        }
        pub fn assign(&self, pid: u32) -> Result<(), std::io::Error> {
            unsafe {
                let process = OpenProcess(0x101, 0, pid);
                if process.is_null() {
                    return Err(std::io::Error::last_os_error());
                }
                let result = AssignProcessToJobObject(self.0 as *mut c_void, process);
                let error = std::io::Error::last_os_error();
                CloseHandle(process);
                if result == 0 {
                    Err(error)
                } else {
                    Ok(())
                }
            }
        }
    }
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0 as *mut c_void);
            }
        }
    }
}

/// Returns true when shutdown was requested during the wait.
async fn wait_or_shutdown(shutdown: &mut watch::Receiver<bool>, delay: Duration) -> bool {
    if *shutdown.borrow() {
        return true;
    }
    tokio::select! {
        _ = tokio::time::sleep(delay) => *shutdown.borrow(),
        result = shutdown.changed() => result.is_err() || *shutdown.borrow(),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    #[test]
    fn only_orphaned_cores_on_our_config_are_stale() {
        let dir = std::path::Path::new("/Users/u/Library/Application Support/Purr");
        let bin =
            std::path::Path::new("/Applications/Purr.app/Contents/Resources/mihomo-darwin-arm64");
        let ps = "\
 8251     1 /Users/u/Library/Application Support/Purr/core/mihomo-darwin-arm64 -d /Users/u/Library/Application Support/Purr -f /Users/u/Library/Application Support/Purr/config.yaml
10596 10546 /Users/u/Library/Application Support/Purr/core/mihomo-darwin-arm64 -d /Users/u/Library/Application Support/Purr -f /Users/u/Library/Application Support/Purr/config.yaml
  700     1 /opt/other/mihomo-darwin-arm64 -d /tmp/elsewhere -f /tmp/elsewhere/config.yaml
  701     1 /usr/bin/python3 -d /Users/u/Library/Application Support/Purr -f x
  702     1 /Applications/FlClash.app/Contents/MacOS/FlClashCore
";
        assert_eq!(stale_core_pids(ps, bin, dir), vec![8251]);
    }
}
