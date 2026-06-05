use std::path::Path;

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

use crate::error::{PaleError, PaleResult};

/// A persisted call record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallRecord {
    pub id: i64,
    pub direction: String,
    pub remote_uri: String,
    pub remote_name: String,
    pub start_time: String, // ISO 8601
    pub duration_secs: i64,
    pub answered: bool,
}

/// Manages call history in a local SQLite database
pub struct CallHistoryDb {
    conn: Connection,
}

impl CallHistoryDb {
    /// Open (or create) the call history database at the given path
    pub fn open(db_path: &Path) -> PaleResult<Self> {
        let conn = Connection::open(db_path)
            .map_err(|e| PaleError::Pjsip(format!("Failed to open call history DB: {}", e), -1))?;

        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS call_history (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                direction   TEXT NOT NULL,
                remote_uri  TEXT NOT NULL,
                remote_name TEXT NOT NULL DEFAULT '',
                start_time  TEXT NOT NULL,
                duration_secs INTEGER NOT NULL DEFAULT 0,
                answered    INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_start_time ON call_history(start_time DESC);",
        )
        .map_err(|e| PaleError::Pjsip(format!("Failed to create tables: {}", e), -1))?;

        Ok(Self { conn })
    }

    /// Insert a new call record
    pub fn insert(&self, record: &CallRecord) -> PaleResult<i64> {
        self.conn
            .execute(
                "INSERT INTO call_history (direction, remote_uri, remote_name, start_time, duration_secs, answered)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    record.direction,
                    record.remote_uri,
                    record.remote_name,
                    record.start_time,
                    record.duration_secs,
                    record.answered as i32,
                ],
            )
            .map_err(|e| PaleError::Pjsip(format!("Failed to insert call record: {}", e), -1))?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Get recent call records (most recent first)
    pub fn list_recent(&self, limit: u32) -> PaleResult<Vec<CallRecord>> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, direction, remote_uri, remote_name, start_time, duration_secs, answered
                 FROM call_history ORDER BY start_time DESC LIMIT ?1",
            )
            .map_err(|e| PaleError::Pjsip(format!("Query failed: {}", e), -1))?;

        let records = stmt
            .query_map(params![limit], |row| {
                Ok(CallRecord {
                    id: row.get(0)?,
                    direction: row.get(1)?,
                    remote_uri: row.get(2)?,
                    remote_name: row.get(3)?,
                    start_time: row.get(4)?,
                    duration_secs: row.get(5)?,
                    answered: row.get::<_, i32>(6)? != 0,
                })
            })
            .map_err(|e| PaleError::Pjsip(format!("Query failed: {}", e), -1))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(records)
    }

    /// Delete a single call record
    pub fn delete(&self, id: i64) -> PaleResult<()> {
        self.conn
            .execute("DELETE FROM call_history WHERE id = ?1", params![id])
            .map_err(|e| PaleError::Pjsip(format!("Delete failed: {}", e), -1))?;
        Ok(())
    }

    /// Clear all call history
    pub fn clear_all(&self) -> PaleResult<()> {
        self.conn
            .execute("DELETE FROM call_history", [])
            .map_err(|e| PaleError::Pjsip(format!("Clear failed: {}", e), -1))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_call_history_crud() {
        let db = CallHistoryDb::open(Path::new(":memory:")).unwrap();

        let record = CallRecord {
            id: 0,
            direction: "outbound".to_string(),
            remote_uri: "sip:alice@example.com".to_string(),
            remote_name: "Alice Smith".to_string(),
            start_time: "2026-06-04T12:00:00Z".to_string(),
            duration_secs: 120,
            answered: true,
        };

        let id = db.insert(&record).unwrap();
        assert!(id > 0);

        let records = db.list_recent(10).unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].remote_name, "Alice Smith");
        assert_eq!(records[0].duration_secs, 120);

        db.delete(id).unwrap();
        let records = db.list_recent(10).unwrap();
        assert_eq!(records.len(), 0);
    }
}
