use rusqlite::{Connection, Result, params};
use std::time::SystemTime;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadRecord {
    pub id: i64,
    pub url: String,
    pub file_path: String,
    pub file_name: String,
    pub mime: Option<String>,
    pub total_bytes: i64,
    pub received_bytes: i64,
    pub state: String, // "downloading", "completed", "cancelled", "failed", "interrupted"
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub error: Option<String>,
}

pub struct DownloadRepository<'a> {
    conn: &'a Connection,
}

impl<'a> DownloadRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Inserts a new download record. Returns generated id, or -1 if persistence is disallowed.
    pub fn insert(
        &self,
        url: &str,
        file_path: &str,
        file_name: &str,
        mime: Option<&str>,
        total_bytes: i64,
        persistence_allowed: bool,
    ) -> Result<i64> {
        if !persistence_allowed {
            return Ok(-1);
        }

        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        self.conn.execute(
            "INSERT INTO downloads (url, file_path, file_name, mime, total_bytes, received_bytes, state, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 'downloading', ?6);",
            params![url, file_path, file_name, mime, total_bytes, now],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Updates received and total bytes
    pub fn update_progress(
        &self,
        id: i64,
        received_bytes: i64,
        total_bytes: i64,
        persistence_allowed: bool,
    ) -> Result<()> {
        if !persistence_allowed || id <= 0 {
            return Ok(());
        }

        self.conn.execute(
            "UPDATE downloads SET received_bytes = ?1, total_bytes = ?2 WHERE id = ?3;",
            params![received_bytes, total_bytes, id],
        )?;

        Ok(())
    }

    /// Updates download terminal or interim state
    pub fn update_state(
        &self,
        id: i64,
        state: &str,
        finished: bool,
        error: Option<&str>,
        persistence_allowed: bool,
    ) -> Result<()> {
        if !persistence_allowed || id <= 0 {
            return Ok(());
        }

        let finished_at = if finished {
            Some(
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64,
            )
        } else {
            None
        };

        self.conn.execute(
            "UPDATE downloads SET state = ?1, finished_at = ?2, error = ?3 WHERE id = ?4;",
            params![state, finished_at, error, id],
        )?;

        Ok(())
    }

    /// Marks any lingering "downloading" states as "interrupted" on application startup.
    /// If the file on disk is already fully written (size matches total_bytes), marks it "completed".
    pub fn mark_interrupted_on_startup(&self) -> Result<usize> {
        let mut stmt = self.conn.prepare(
            "SELECT id, file_path, total_bytes FROM downloads WHERE state = 'downloading';",
        )?;
        let rows: Vec<(i64, String, i64)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?
            .filter_map(|r| r.ok())
            .collect();

        for (id, file_path, total_bytes) in rows {
            let path = std::path::Path::new(&file_path);
            if total_bytes > 0
                && let Ok(meta) = std::fs::metadata(path)
                && meta.len() >= total_bytes as u64
            {
                let _ = self.conn.execute(
                    "UPDATE downloads SET state = 'completed', received_bytes = ?1, error = NULL WHERE id = ?2;",
                    rusqlite::params![total_bytes, id],
                );
            }
        }

        self.conn.execute(
            "UPDATE downloads SET state = 'interrupted', error = 'Download interrupted by browser restart'
             WHERE state = 'downloading';",
            [],
        )
    }

    /// Fetches recent downloads
    pub fn get_recent(&self, limit: usize) -> Result<Vec<DownloadRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, url, file_path, file_name, mime, total_bytes, received_bytes, state, started_at, finished_at, error
             FROM downloads
             ORDER BY started_at DESC
             LIMIT ?1;",
        )?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(DownloadRecord {
                id: row.get(0)?,
                url: row.get(1)?,
                file_path: row.get(2)?,
                file_name: row.get(3)?,
                mime: row.get(4)?,
                total_bytes: row.get(5)?,
                received_bytes: row.get(6)?,
                state: row.get(7)?,
                started_at: row.get(8)?,
                finished_at: row.get(9)?,
                error: row.get(10)?,
            })
        })?;

        let mut list = Vec::new();
        for item in rows {
            list.push(item?);
        }
        Ok(list)
    }

    /// Deletes a download record
    pub fn delete(&self, id: i64) -> Result<()> {
        self.conn
            .execute("DELETE FROM downloads WHERE id = ?1;", params![id])?;
        Ok(())
    }

    /// Clears all completed, cancelled, or failed downloads from history
    pub fn clear_finished(&self) -> Result<usize> {
        self.conn.execute(
            "DELETE FROM downloads WHERE state IN ('completed', 'cancelled', 'failed', 'interrupted');",
            [],
        )
    }

    /// Clears download entries started at or after `cutoff` (or all if cutoff <= 0)
    pub fn clear_since(&self, cutoff: i64) -> Result<usize> {
        if cutoff <= 0 {
            self.conn.execute("DELETE FROM downloads;", [])
        } else {
            self.conn.execute(
                "DELETE FROM downloads WHERE started_at >= ?1;",
                params![cutoff],
            )
        }
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use crate::storage::database::Database;

    #[test]
    fn test_download_lifecycle_and_interruption() {
        let db = Database::open_in_memory().unwrap();
        let repo = DownloadRepository::new(db.conn());

        let id = repo
            .insert(
                "https://example.com/file.tar.gz",
                "/home/user/Downloads/file.tar.gz",
                "file.tar.gz",
                Some("application/gzip"),
                1000,
                true,
            )
            .unwrap();
        assert!(id > 0);

        repo.update_progress(id, 500, 1000, true).unwrap();

        let recent = repo.get_recent(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].received_bytes, 500);
        assert_eq!(recent[0].state, "downloading");

        // Simulate crash restart
        let interrupted = repo.mark_interrupted_on_startup().unwrap();
        assert_eq!(interrupted, 1);

        let recent_after = repo.get_recent(10).unwrap();
        assert_eq!(recent_after[0].state, "interrupted");
    }

    #[test]
    fn test_download_private_mode_persistence_flag() {
        let db = Database::open_in_memory().unwrap();
        let repo = DownloadRepository::new(db.conn());

        let id = repo
            .insert(
                "https://example.com/secret.pdf",
                "/home/user/Downloads/secret.pdf",
                "secret.pdf",
                Some("application/pdf"),
                500,
                false, // persistence forbidden
            )
            .unwrap();
        assert_eq!(id, -1);

        repo.update_progress(-1, 200, 500, false).unwrap();
        repo.update_state(-1, "completed", true, None, false)
            .unwrap();

        let count: i64 = db
            .conn()
            .query_row("SELECT COUNT(*) FROM downloads;", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            count, 0,
            "No records must be written when persistence_allowed is false"
        );
    }
}
