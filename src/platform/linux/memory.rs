//! System memory metrics reader for Linux
//!
//! Reads real memory statistics from `/proc/meminfo` to detect memory pressure.

use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub available_kb: u64,
}

impl MemoryInfo {
    /// Reads real system memory stats from `/proc/meminfo`
    pub fn read_current() -> Option<Self> {
        let file = File::open("/proc/meminfo").ok()?;
        let reader = BufReader::new(file);

        let mut total_kb = None;
        let mut available_kb = None;

        for line in reader.lines().map_while(Result::ok) {
            if line.starts_with("MemTotal:") {
                total_kb = parse_meminfo_kb(&line);
            } else if line.starts_with("MemAvailable:") {
                available_kb = parse_meminfo_kb(&line);
            }

            if total_kb.is_some() && available_kb.is_some() {
                break;
            }
        }

        Some(Self {
            total_kb: total_kb?,
            available_kb: available_kb?,
        })
    }

    /// Determines if system is under severe memory pressure:
    /// Returns true if available memory is less than 10% of total OR less than 350 MB.
    pub fn is_under_pressure(&self) -> bool {
        if self.total_kb == 0 {
            return false;
        }
        let available_mb = self.available_kb / 1024;
        let percent_available = (self.available_kb as f64 / self.total_kb as f64) * 100.0;
        available_mb < 350 || percent_available < 10.0
    }
}

fn parse_meminfo_kb(line: &str) -> Option<u64> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() >= 2 {
        parts[1].parse::<u64>().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_meminfo_kb() {
        assert_eq!(parse_meminfo_kb("MemTotal:       16274488 kB"), Some(16274488));
        assert_eq!(parse_meminfo_kb("MemAvailable:    8765432 kB"), Some(8765432));
        assert_eq!(parse_meminfo_kb("Invalid line"), None);
    }

    #[test]
    fn test_is_under_pressure() {
        let normal = MemoryInfo {
            total_kb: 16_000_000,
            available_kb: 8_000_000,
        };
        assert!(!normal.is_under_pressure());

        let low_percent = MemoryInfo {
            total_kb: 16_000_000,
            available_kb: 1_200_000, // < 10%
        };
        assert!(low_percent.is_under_pressure());

        let low_mb = MemoryInfo {
            total_kb: 4_000_000,
            available_kb: 300_000, // < 350 MB
        };
        assert!(low_mb.is_under_pressure());
    }
}
