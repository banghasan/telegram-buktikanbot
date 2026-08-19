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
        user_name: String,
        user_username: Option<String>,
        chat_title: Option<String>,
        chat_username: Option<String>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            conn.execute(
                "INSERT INTO ban_release_jobs
                 (chat_id, user_id, release_at, user_name, user_username, chat_title, chat_username)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(chat_id, user_id) DO UPDATE SET
                    release_at=excluded.release_at,
                    user_name=excluded.user_name,
                    user_username=excluded.user_username,
                    chat_title=excluded.chat_title,
                    chat_username=excluded.chat_username",
                params![
                    chat_id,
                    user_id,
                    release_at,
                    user_name,
                    user_username,
                    chat_title,
                    chat_username
                ],
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn fetch_due(
        &self,
        now_ts: i64,
    ) -> Result<Vec<BanReleaseJob>, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            let mut stmt = conn.prepare(
                "SELECT chat_id, user_id, release_at, user_name, user_username, chat_title, chat_username
                 FROM ban_release_jobs
                 WHERE release_at <= ?1
                 ORDER BY release_at ASC",
            )?;
            let rows = stmt.query_map([now_ts], |row| {
                Ok(BanReleaseJob {
                    chat_id: row.get(0)?,
                    user_id: row.get(1)?,
                    release_at: row.get(2)?,
                    user_name: row.get(3)?,
                    user_username: row.get(4)?,
                    chat_title: row.get(5)?,
                    chat_username: row.get(6)?,
                })
            })?;
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
            user_name TEXT NOT NULL,
            user_username TEXT,
            chat_title TEXT,
            chat_username TEXT,
            PRIMARY KEY (chat_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_ban_release_jobs_release_at
            ON ban_release_jobs (release_at);",
    )?;
    conn.execute_batch(
        "ALTER TABLE ban_release_jobs ADD COLUMN user_name TEXT;
         ALTER TABLE ban_release_jobs ADD COLUMN user_username TEXT;
         ALTER TABLE ban_release_jobs ADD COLUMN chat_title TEXT;
         ALTER TABLE ban_release_jobs ADD COLUMN chat_username TEXT;",
    )
    .ok();
    conn.execute_batch(
        "UPDATE ban_release_jobs
         SET user_name = COALESCE(user_name, '-')
         WHERE user_name IS NULL;",
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

#[derive(Debug, Clone)]
pub struct BanReleaseJob {
    pub chat_id: i64,
    pub user_id: i64,
    pub release_at: i64,
    pub user_name: String,
    pub user_username: Option<String>,
    pub chat_title: Option<String>,
    pub chat_username: Option<String>,
}
