//! Verification of WebKit child process sandboxing, seccomp status, and namespaces
//!
//! Inspects `/proc` for child `WebKitWebProcess` instances, verifying
//! whether they are wrapped with Bubblewrap (`bwrap`), running under an ancestor `bwrap`,
//! enforcing Seccomp filter mode (`Seccomp: 2`), or isolated within distinct namespaces.

use crate::platform::traits::{PlatformSandboxStatus, SandboxChecker};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SandboxStatus {
    Active,
    Inactive,
    NoChildProcessYet,
}

impl From<PlatformSandboxStatus> for SandboxStatus {
    fn from(status: PlatformSandboxStatus) -> Self {
        match status {
            PlatformSandboxStatus::Active => SandboxStatus::Active,
            PlatformSandboxStatus::Inactive => SandboxStatus::Inactive,
            PlatformSandboxStatus::NoChildProcessYet => SandboxStatus::NoChildProcessYet,
        }
    }
}

/// Inspects descendant processes in `/proc` belonging to the current process,
/// checking if any WebKitWebProcess child has an active seccomp filter, bwrap wrapper, or isolated namespaces.
pub fn check_webkit_child_sandbox() -> SandboxStatus {
    #[cfg(target_os = "linux")]
    {
        use crate::platform::linux::LinuxSandboxChecker;
        let checker = LinuxSandboxChecker::new();
        checker.check_sandbox_status().into()
    }
    #[cfg(not(target_os = "linux"))]
    {
        SandboxStatus::Active
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
