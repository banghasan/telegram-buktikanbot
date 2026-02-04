use std::error::Error;
use std::time::Duration;

use rusqlite::{Connection, params};

#[derive(Clone)]
pub struct BanReleaseStore {
    db_path: String,
}

impl BanReleaseStore {
    pub async fn init(db_path: String) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let store = Self { db_path };
        let path = store.db_path.clone();
        tokio::task::spawn_blocking(move || init_db(&path))
            .await?
            .map_err(|err| -> Box<dyn Error + Send + Sync> { err.into() })?;
        Ok(store)
    }

    pub async fn upsert_job(
        &self,
        chat_id: i64,
        user_id: i64,
        release_at: i64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            conn.execute(
                "INSERT INTO ban_release_jobs (chat_id, user_id, release_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(chat_id, user_id) DO UPDATE SET release_at=excluded.release_at",
                params![chat_id, user_id, release_at],
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn fetch_due(
        &self,
        now_ts: i64,
    ) -> Result<Vec<(i64, i64, i64)>, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            let mut stmt = conn.prepare(
                "SELECT chat_id, user_id, release_at
                 FROM ban_release_jobs
                 WHERE release_at <= ?1
                 ORDER BY release_at ASC",
            )?;
            let rows =
                stmt.query_map([now_ts], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok::<_, rusqlite::Error>(out)
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn delete_job(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            conn.execute(
                "DELETE FROM ban_release_jobs WHERE chat_id = ?1 AND user_id = ?2",
                params![chat_id, user_id],
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await?
        .map_err(|err| err.into())
    }
}

fn init_db(path: &str) -> Result<(), rusqlite::Error> {
    let conn = open_db(path)?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS ban_release_jobs (
            chat_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            release_at INTEGER NOT NULL,
            PRIMARY KEY (chat_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_ban_release_jobs_release_at
            ON ban_release_jobs (release_at);",
    )?;
    Ok(())
}

fn open_db(path: &str) -> Result<Connection, rusqlite::Error> {
    let conn = Connection::open(path)?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "busy_timeout", "3000")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(conn)
}

pub fn worker_interval() -> Duration {
    Duration::from_secs(60)
}
