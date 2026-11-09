//! Linux WebKit child process and bubblewrap sandbox inspection
//!
//! Checks descendant processes in `/proc` for seccomp mode and PID/user namespaces.

use crate::platform::traits::{PlatformSandboxStatus, SandboxChecker};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

pub struct LinuxSandboxChecker;

impl Default for LinuxSandboxChecker {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxSandboxChecker {
    pub fn new() -> Self {
        Self
    }
}

struct ProcMetadata {
    pid: u32,
    ppid: u32,
    name: String,
    seccomp: u32,
}

impl SandboxChecker for LinuxSandboxChecker {
    fn check_sandbox_status(&self) -> PlatformSandboxStatus {
        let my_pid = std::process::id();
        let proc_dir = Path::new("/proc");

        let entries = match fs::read_dir(proc_dir) {
            Ok(e) => e,
            Err(_) => return PlatformSandboxStatus::Inactive,
        };

        let mut procs: Vec<ProcMetadata> = Vec::new();

        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let pid_str = file_name.to_string_lossy();
            if !pid_str.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }

            let pid: u32 = match pid_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            let status_path = entry.path().join("status");
            if let Ok(content) = fs::read_to_string(status_path) {
                let mut ppid: Option<u32> = None;
                let mut name: Option<String> = None;
                let mut seccomp: Option<u32> = None;

                for line in content.lines() {
                    if let Some(rest) = line.strip_prefix("PPid:") {
                        ppid = rest.trim().parse::<u32>().ok();
                    } else if let Some(rest) = line.strip_prefix("Name:") {
                        name = Some(rest.trim().to_string());
                    } else if let Some(rest) = line.strip_prefix("Seccomp:") {
                        seccomp = rest.trim().parse::<u32>().ok();
                    }
                }

                if let (Some(ppid), Some(name)) = (ppid, name) {
                    procs.push(ProcMetadata {
                        pid,
                        ppid,
                        name,
                        seccomp: seccomp.unwrap_or(0),
                    });
                }
            }
        }

        // Traverse process tree to find all descendants of the UI process
        let mut descendant_pids = HashSet::new();
        let mut queue = vec![my_pid];

        while let Some(parent) = queue.pop() {
            for p in &procs {
                if p.ppid == parent && !descendant_pids.contains(&p.pid) {
                    descendant_pids.insert(p.pid);
                    queue.push(p.pid);
                }
            }
        }

        // Read UI process namespaces for comparison
        let my_pid_ns = fs::read_link(format!("/proc/{my_pid}/ns/pid")).ok();
        let my_user_ns = fs::read_link(format!("/proc/{my_pid}/ns/user")).ok();

        let mut found_web_process = false;

        for p in &procs {
            if descendant_pids.contains(&p.pid)
                && (p.name.contains("WebKitWebProcess")
                    || p.name.contains("WebProcess")
                    || p.name.contains("WebKitNetworkPr"))
            {
                found_web_process = true;

                // Seccomp mode 2 indicates filter is active
                let has_seccomp = p.seccomp >= 2;

                // Check namespace isolation
                let child_pid_ns = fs::read_link(format!("/proc/{}/ns/pid", p.pid)).ok();
                let child_user_ns = fs::read_link(format!("/proc/{}/ns/user", p.pid)).ok();

                let has_isolated_pid_ns = match (&my_pid_ns, &child_pid_ns) {
                    (Some(parent), Some(child)) => parent != child,
                    _ => false,
                };

                let has_isolated_user_ns = match (&my_user_ns, &child_user_ns) {
                    (Some(parent), Some(child)) => parent != child,
                    _ => false,
                };

                // Active sandbox when seccomp filter is active and namespaces are isolated
                if has_seccomp && (has_isolated_pid_ns || has_isolated_user_ns) {
                    return PlatformSandboxStatus::Active;
                }
            }
        }

        if found_web_process {
            PlatformSandboxStatus::Inactive
        } else {
            PlatformSandboxStatus::NoChildProcessYet
        }
    }
}
