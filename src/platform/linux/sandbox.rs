//! Linux WebKit child process and bubblewrap sandbox inspection
//!
//! Inspects `/proc` for child `WebKitWebProcess` instances, verifying
//! whether they are wrapped with Bubblewrap (`bwrap`), running under an ancestor `bwrap`,
//! enforcing Seccomp filter mode (`Seccomp: 2`), or isolated within distinct namespaces.

use crate::platform::traits::{PlatformSandboxStatus, SandboxChecker};
use std::collections::{HashMap, HashSet};
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

    /// Verifies if Linux unprivileged user namespaces are enabled by the kernel
    pub fn is_unprivileged_userns_enabled() -> bool {
        if let Ok(content) = fs::read_to_string("/proc/sys/kernel/unprivileged_userns_clone")
            && content.trim() == "0"
        {
            return false;
        }
        if let Ok(content) = fs::read_to_string("/proc/sys/user/max_user_namespaces")
            && let Ok(val) = content.trim().parse::<u64>()
            && val == 0
        {
            return false;
        }
        true
    }
}

#[derive(Clone, Debug)]
struct ProcInfo {
    ppid: u32,
    name: String,
    cmdline: String,
    seccomp: u32,
    uid: u32,
}

impl SandboxChecker for LinuxSandboxChecker {
    fn check_sandbox_status(&self) -> PlatformSandboxStatus {
        let my_pid = std::process::id();
        let my_uid = get_process_uid(my_pid);

        let proc_dir = Path::new("/proc");
        let entries = match fs::read_dir(proc_dir) {
            Ok(e) => e,
            Err(e) => {
                println!("[SandboxChecker] Error opening /proc: {e}");
                return PlatformSandboxStatus::Inactive;
            }
        };

        let mut procs: HashMap<u32, ProcInfo> = HashMap::new();

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

            // Read /proc/<pid>/cmdline
            let cmdline = match fs::read(entry.path().join("cmdline")) {
                Ok(bytes) => String::from_utf8_lossy(&bytes).replace('\0', " "),
                Err(_) => String::new(),
            };

            // Read /proc/<pid>/status
            let mut ppid = 0;
            let mut name = String::new();
            let mut seccomp = 0;
            let mut uid = 0;

            if let Ok(content) = fs::read_to_string(entry.path().join("status")) {
                for line in content.lines() {
                    if let Some(rest) = line.strip_prefix("PPid:") {
                        ppid = rest.trim().parse::<u32>().unwrap_or(0);
                    } else if let Some(rest) = line.strip_prefix("Name:") {
                        name = rest.trim().to_string();
                    } else if let Some(rest) = line.strip_prefix("Seccomp:") {
                        seccomp = rest.trim().parse::<u32>().unwrap_or(0);
                    } else if let Some(rest) = line.strip_prefix("Uid:") {
                        uid = rest
                            .split_whitespace()
                            .next()
                            .and_then(|s| s.parse::<u32>().ok())
                            .unwrap_or(0);
                    }
                }
            }

            procs.insert(
                pid,
                ProcInfo {
                    ppid,
                    name,
                    cmdline,
                    seccomp,
                    uid,
                },
            );
        }

        // Trace descendants of UI process
        let mut descendant_pids = HashSet::new();
        let mut queue = vec![my_pid];

        while let Some(parent) = queue.pop() {
            for (&pid, proc) in &procs {
                if proc.ppid == parent && !descendant_pids.contains(&pid) {
                    descendant_pids.insert(pid);
                    queue.push(pid);
                }
            }
        }

        let my_pid_ns = fs::read_link(format!("/proc/{my_pid}/ns/pid")).ok();
        let my_user_ns = fs::read_link(format!("/proc/{my_pid}/ns/user")).ok();
        let my_mnt_ns = fs::read_link(format!("/proc/{my_pid}/ns/mnt")).ok();
        let my_net_ns = fs::read_link(format!("/proc/{my_pid}/ns/net")).ok();

        let mut found_web_process = false;
        let mut any_sandbox_active = false;
        let mut unsandboxed_procs = Vec::new();

        for (&pid, proc) in &procs {
            // Check if process matches WebKitWebProcess or WebProcess
            let is_web_process = proc.cmdline.contains("WebKitWebProcess")
                || proc.cmdline.contains("WebProcess")
                || proc.name.contains("WebKitWebProces")
                || proc.name.contains("WebProcess");

            if !is_web_process {
                continue;
            }

            // Only consider processes that are either direct/indirect descendants of UI process,
            // or owned by the current user (in case of reparenting/subreaper).
            let is_our_process = descendant_pids.contains(&pid)
                || (my_uid.is_some() && Some(proc.uid) == my_uid);

            if !is_our_process {
                continue;
            }

            found_web_process = true;

            println!(
                "[SandboxChecker] Inspected WebKit process: PID={}, PPid={}, Name='{}', Cmdline='{}', Seccomp={}, UID={}",
                pid, proc.ppid, proc.name, proc.cmdline, proc.seccomp, proc.uid
            );

            // 1. Check if cmdline directly contains bwrap
            if proc.cmdline.contains("bwrap") {
                println!(
                    "[SandboxChecker] -> Sandbox ACTIVE: PID {} command line contains 'bwrap'",
                    pid
                );
                any_sandbox_active = true;
                break;
            }

            // 2. Check if parent or any ancestor process is bwrap
            let mut curr_ppid = proc.ppid;
            let mut ancestor_is_bwrap = false;
            for _ in 0..6 {
                if curr_ppid == 0 || curr_ppid == 1 {
                    break;
                }
                if let Some(parent) = procs.get(&curr_ppid) {
                    if parent.cmdline.contains("bwrap") || parent.name.contains("bwrap") {
                        println!(
                            "[SandboxChecker] -> Sandbox ACTIVE: PID {} ancestor PID {} is 'bwrap' (cmdline='{}')",
                            pid, curr_ppid, parent.cmdline
                        );
                        ancestor_is_bwrap = true;
                        break;
                    }
                    curr_ppid = parent.ppid;
                } else if let Ok(bytes) = fs::read(format!("/proc/{curr_ppid}/cmdline")) {
                    let cmd = String::from_utf8_lossy(&bytes);
                    if cmd.contains("bwrap") {
                        println!(
                            "[SandboxChecker] -> Sandbox ACTIVE: PID {} ancestor PID {} cmdline contains 'bwrap'",
                            pid, curr_ppid
                        );
                        ancestor_is_bwrap = true;
                        break;
                    }
                    break;
                } else {
                    break;
                }
            }

            if ancestor_is_bwrap {
                any_sandbox_active = true;
                break;
            }

            // 3. Check Seccomp mode (2 = SECCOMP_MODE_FILTER)
            if proc.seccomp >= 2 {
                println!(
                    "[SandboxChecker] -> Sandbox ACTIVE: PID {} has active Seccomp BPF filter (Seccomp={})",
                    pid, proc.seccomp
                );
                any_sandbox_active = true;
                break;
            }

            // 4. Check namespace isolation (pid, user, mount, or network namespace)
            let child_pid_ns = fs::read_link(format!("/proc/{pid}/ns/pid")).ok();
            let child_user_ns = fs::read_link(format!("/proc/{pid}/ns/user")).ok();
            let child_mnt_ns = fs::read_link(format!("/proc/{pid}/ns/mnt")).ok();
            let child_net_ns = fs::read_link(format!("/proc/{pid}/ns/net")).ok();

            let ns_isolated = match (&my_pid_ns, &child_pid_ns) {
                (Some(parent), Some(child)) => parent != child,
                _ => false,
            } || match (&my_user_ns, &child_user_ns) {
                (Some(parent), Some(child)) => parent != child,
                _ => false,
            } || match (&my_mnt_ns, &child_mnt_ns) {
                (Some(parent), Some(child)) => parent != child,
                _ => false,
            } || match (&my_net_ns, &child_net_ns) {
                (Some(parent), Some(child)) => parent != child,
                _ => false,
            };

            if ns_isolated {
                println!(
                    "[SandboxChecker] -> Sandbox ACTIVE: PID {} has isolated namespaces (pid_ns={:?}, user_ns={:?}, mnt_ns={:?}, net_ns={:?})",
                    pid, child_pid_ns, child_user_ns, child_mnt_ns, child_net_ns
                );
                any_sandbox_active = true;
                break;
            }

            // If reading namespace returned PermissionDenied while we could read our own, it's inside user namespace
            if my_pid_ns.is_some()
                && let Err(e) = fs::read_link(format!("/proc/{pid}/ns/pid"))
                && e.kind() == std::io::ErrorKind::PermissionDenied
            {
                println!(
                    "[SandboxChecker] -> Sandbox ACTIVE: PID {} namespace inspection restricted (user namespace boundary)",
                    pid
                );
                any_sandbox_active = true;
                break;
            }

            unsandboxed_procs.push((pid, proc.cmdline.clone(), proc.ppid, proc.seccomp));
        }

        if any_sandbox_active {
            println!("[SandboxChecker] Overall result: Active");
            PlatformSandboxStatus::Active
        } else if found_web_process {
            for (pid, cmdline, ppid, seccomp) in &unsandboxed_procs {
                tracing::warn!(
                    "[SandboxChecker] WebKit process PID {} is un-sandboxed! PPid: {}, Seccomp: {}, Cmdline: '{}'",
                    pid, ppid, seccomp, cmdline
                );
                println!(
                    "[SandboxChecker] WARNING: WebKit process PID {} is un-sandboxed! PPid: {}, Seccomp: {}, Cmdline: '{}'",
                    pid, ppid, seccomp, cmdline
                );
            }
            if !Self::is_unprivileged_userns_enabled() {
                tracing::warn!("[SandboxChecker] Kernel unprivileged user namespaces are disabled!");
            }
            PlatformSandboxStatus::Inactive
        } else {
            println!("[SandboxChecker] Overall result: NoChildProcessYet");
            PlatformSandboxStatus::NoChildProcessYet
        }
    }
}

fn get_process_uid(pid: u32) -> Option<u32> {
    let content = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Uid:") {
            return rest.split_whitespace().next().and_then(|s| s.parse().ok());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linux_sandbox_checker_live() {
        let checker = LinuxSandboxChecker::new();
        let status = checker.check_sandbox_status();
        println!("Test live sandbox status: {:?}", status);
        assert!(matches!(
            status,
            PlatformSandboxStatus::Active | PlatformSandboxStatus::NoChildProcessYet
        ));
    }

    #[test]
    fn test_unprivileged_userns_check() {
        let enabled = LinuxSandboxChecker::is_unprivileged_userns_enabled();
        println!("Unprivileged user namespaces enabled: {enabled}");
        assert!(enabled);
    }
}
