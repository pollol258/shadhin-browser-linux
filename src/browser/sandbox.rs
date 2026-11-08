//! Verification of WebKit child process sandboxing, seccomp status, and namespaces
//!
//! Checks the actual child `WebKitWebProcess` processes spawned by the UI process
//! (including grandchildren inside bubblewrap `bwrap` containers), inspecting
//! `/proc/<pid>/status` for Seccomp mode and `/proc/<pid>/ns/*` for namespace isolation.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxStatus {
    Active,
    Inactive,
    NoChildProcessYet,
}

struct ProcMetadata {
    pid: u32,
    ppid: u32,
    name: String,
    seccomp: u32,
}

/// Inspects descendant processes in `/proc` belonging to the current process,
/// checking if any WebKitWebProcess child has an active seccomp filter and isolated namespaces.
pub fn check_webkit_child_sandbox() -> SandboxStatus {
    let my_pid = std::process::id();
    let proc_dir = Path::new("/proc");

    let entries = match fs::read_dir(proc_dir) {
        Ok(e) => e,
        Err(_) => return SandboxStatus::Inactive,
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

            // 1. Seccomp check: 2 = SECCOMP_MODE_FILTER, 1 = SECCOMP_MODE_STRICT
            let seccomp_active = p.seccomp > 0;

            // 2. Namespace check: bubblewrap or user namespace isolation
            let child_pid_ns = fs::read_link(format!("/proc/{}/ns/pid", p.pid)).ok();
            let child_user_ns = fs::read_link(format!("/proc/{}/ns/user", p.pid)).ok();

            let ns_isolated = (child_pid_ns.is_some() && child_pid_ns != my_pid_ns)
                || (child_user_ns.is_some() && child_user_ns != my_user_ns);

            if seccomp_active || ns_isolated {
                return SandboxStatus::Active;
            }
        }
    }

    if found_web_process {
        SandboxStatus::Inactive
    } else {
        SandboxStatus::NoChildProcessYet
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_check_does_not_panic() {
        let status = check_webkit_child_sandbox();
        assert!(matches!(
            status,
            SandboxStatus::Active | SandboxStatus::Inactive | SandboxStatus::NoChildProcessYet
        ));
    }
}
