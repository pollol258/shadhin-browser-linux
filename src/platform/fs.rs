//! Cross-platform secure filesystem permission helpers
//!
//! Enforces restrictive permissions (0700 for profile/key directories, 0600 for locks/db,
//! 0644 for downloaded files) on Unix/Linux, while remaining fully compile-safe on non-Unix platforms.

use std::fs::{self, DirBuilder, File, OpenOptions};
use std::io::Result;
use std::path::Path;

/// Creates a directory with restrictive permissions (mode 0700 on Unix)
pub fn create_secure_directory<P: AsRef<Path>>(path: P) -> Result<()> {
    let path = path.as_ref();
    if path.exists() {
        return Ok(());
    }

    let mut builder = DirBuilder::new();
    builder.recursive(true);

    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }

    builder.create(path)
}

/// Sets permissions of a file to private / restricted (mode 0600 on Unix)
pub fn set_file_private<P: AsRef<Path>>(path: P) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path.as_ref())?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o600);
        fs::set_permissions(path.as_ref(), permissions)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Sets permissions of a file to standard / user-read-write (mode 0644 on Unix)
pub fn set_file_standard<P: AsRef<Path>>(path: P) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(path.as_ref())?;
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o644);
        fs::set_permissions(path.as_ref(), permissions)?;
    }
    #[cfg(not(unix))]
    {
        let _ = path;
    }
    Ok(())
}

/// Opens or creates a file with private permissions (0600 on Unix)
pub fn open_private_file<P: AsRef<Path>>(path: P, write: bool, create: bool) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    if write {
        options.write(true);
    }
    if create {
        options.create(true);
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }

    options.open(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_secure_dir_and_file_permissions() {
        let sec_dir = std::env::temp_dir().join(format!("sec_test_dir_{}", std::process::id()));
        let _ = fs::remove_dir_all(&sec_dir);
        create_secure_directory(&sec_dir).unwrap();
        assert!(sec_dir.is_dir());

        let sec_file = sec_dir.join("test_file.txt");
        fs::write(&sec_file, b"content").unwrap();
        set_file_private(&sec_file).unwrap();

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&sec_file).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = fs::remove_dir_all(&sec_dir);
    }
}
