use std::error::Error;
use std::time::Duration;

use rusqlite::types::Type;
use rusqlite::{Connection, params};

use crate::captcha::CaptchaSession;

const RELEASE_BATCH_SIZE: i64 = 100;

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

    pub async fn upsert_job(&self, job: BanReleaseJob) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), rusqlite::Error> {
            let conn = open_db(&path)?;
            conn.execute(
                "INSERT INTO ban_release_jobs
                 (chat_id, user_id, release_at, user_name, user_username, chat_title, chat_username,
                  log_chat_id, log_message_thread_id, log_message_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
                 ON CONFLICT(chat_id, user_id) DO UPDATE SET
                    release_at=excluded.release_at,
                    user_name=excluded.user_name,
                    user_username=excluded.user_username,
                    chat_title=excluded.chat_title,
                    chat_username=excluded.chat_username,
                    log_chat_id=excluded.log_chat_id,
                    log_message_thread_id=excluded.log_message_thread_id,
                    log_message_id=excluded.log_message_id",
                params![
                    job.chat_id,
                    job.user_id,
                    job.release_at,
                    job.user_name,
                    job.user_username,
                    job.chat_title,
                    job.chat_username,
                    job.log_chat_id,
                    job.log_message_thread_id,
                    job.log_message_id,
                ],
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn attach_log_message(
        &self,
        chat_id: i64,
        user_id: i64,
        log_chat_id: i64,
        log_message_thread_id: Option<i32>,
        log_message_id: i32,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || -> Result<(), rusqlite::Error> {
            let conn = open_db(&path)?;
            conn.execute(
                "UPDATE ban_release_jobs
                 SET log_chat_id = ?1,
                     log_message_thread_id = ?2,
                     log_message_id = ?3
                 WHERE chat_id = ?4 AND user_id = ?5",
                params![
                    log_chat_id,
                    log_message_thread_id,
                    log_message_id,
                    chat_id,
                    user_id,
                ],
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn save_captcha_session(
        &self,
        session: CaptchaSession,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        let options_json = serde_json::to_string(&session.options)?;
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            conn.execute(
                "INSERT INTO captcha_sessions
                 (chat_id, user_id, code, captcha_message_id, options_json, attempts_left,
                  attempts_total, expires_at, user_first_name, user_last_name, user_username,
                  chat_title, chat_username)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
                 ON CONFLICT(chat_id, user_id) DO UPDATE SET
                    code=excluded.code,
                    captcha_message_id=excluded.captcha_message_id,
                    options_json=excluded.options_json,
                    attempts_left=excluded.attempts_left,
                    attempts_total=excluded.attempts_total,
                    expires_at=excluded.expires_at,
                    user_first_name=excluded.user_first_name,
                    user_last_name=excluded.user_last_name,
                    user_username=excluded.user_username,
                    chat_title=excluded.chat_title,
                    chat_username=excluded.chat_username",
                params![
                    session.chat_id,
                    session.user_id,
                    session.code,
                    session.captcha_message_id,
                    options_json,
                    session.attempts_left,
                    session.attempts_total,
                    session.expires_at,
                    session.user_first_name,
                    session.user_last_name,
                    session.user_username,
                    session.chat_title,
                    session.chat_username,
                ],
            )?;
            Ok::<_, rusqlite::Error>(())
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn fetch_captcha_sessions(
        &self,
    ) -> Result<Vec<CaptchaSession>, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            let mut stmt = conn.prepare(
                "SELECT chat_id, user_id, code, captcha_message_id, options_json,
                        attempts_left, attempts_total, expires_at, user_first_name,
                        user_last_name, user_username, chat_title, chat_username
                 FROM captcha_sessions
                 ORDER BY expires_at ASC",
            )?;
            let rows = stmt.query_map([], |row| {
                let options_json: String = row.get(4)?;
                let options = serde_json::from_str(&options_json).map_err(|err| {
                    rusqlite::Error::FromSqlConversionFailure(4, Type::Text, Box::new(err))
                })?;
                Ok(CaptchaSession {
                    chat_id: row.get(0)?,
                    user_id: row.get(1)?,
                    code: row.get(2)?,
                    captcha_message_id: row.get(3)?,
                    options,
                    attempts_left: row.get(5)?,
                    attempts_total: row.get(6)?,
                    expires_at: row.get(7)?,
                    user_first_name: row.get(8)?,
                    user_last_name: row.get(9)?,
                    user_username: row.get(10)?,
                    chat_title: row.get(11)?,
                    chat_username: row.get(12)?,
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

    pub async fn delete_captcha_session(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            conn.execute(
                "DELETE FROM captcha_sessions WHERE chat_id = ?1 AND user_id = ?2",
                params![chat_id, user_id],
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
                "SELECT chat_id, user_id, release_at, user_name, user_username, chat_title, chat_username,
                        log_chat_id, log_message_thread_id, log_message_id
                 FROM ban_release_jobs
                 WHERE release_at <= ?1
                 ORDER BY release_at ASC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![now_ts, RELEASE_BATCH_SIZE], |row| {
                ban_release_job_from_row(row)
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

    pub async fn fetch_pending(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<BanReleaseJob>, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        let offset = offset.max(0);
        let limit = limit.clamp(1, 100);
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            let mut stmt = conn.prepare(
                "SELECT chat_id, user_id, release_at, user_name, user_username, chat_title, chat_username,
                        log_chat_id, log_message_thread_id, log_message_id
                 FROM ban_release_jobs
                 ORDER BY release_at ASC, chat_id ASC, user_id ASC
                 LIMIT ?1 OFFSET ?2",
            )?;
            let rows = stmt.query_map(params![limit, offset], ban_release_job_from_row)?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok::<_, rusqlite::Error>(out)
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn count_pending(&self) -> Result<i64, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || -> Result<i64, rusqlite::Error> {
            let conn = open_db(&path)?;
            conn.query_row("SELECT COUNT(*) FROM ban_release_jobs", [], |row| {
                row.get(0)
            })
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn get_job(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<Option<BanReleaseJob>, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || -> Result<Option<BanReleaseJob>, rusqlite::Error> {
            let conn = open_db(&path)?;
            let mut stmt = conn.prepare(
                "SELECT chat_id, user_id, release_at, user_name, user_username, chat_title, chat_username,
                        log_chat_id, log_message_thread_id, log_message_id
                 FROM ban_release_jobs
                 WHERE chat_id = ?1 AND user_id = ?2",
            )?;
            let mut rows = stmt.query(params![chat_id, user_id])?;
            match rows.next()? {
                Some(row) => Ok(Some(ban_release_job_from_row(row)?)),
                None => Ok(None),
            }
        })
        .await?
        .map_err(|err| err.into())
    }

    pub async fn delete_job(
        &self,
        chat_id: i64,
        user_id: i64,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        let path = self.db_path.clone();
        tokio::task::spawn_blocking(move || {
            let conn = open_db(&path)?;
            let deleted = conn.execute(
                "DELETE FROM ban_release_jobs WHERE chat_id = ?1 AND user_id = ?2",
                params![chat_id, user_id],
            )?;
            Ok::<_, rusqlite::Error>(deleted > 0)
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
            log_chat_id INTEGER,
            log_message_thread_id INTEGER,
            log_message_id INTEGER,
            PRIMARY KEY (chat_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_ban_release_jobs_release_at
            ON ban_release_jobs (release_at);",
    )?;
    ensure_column(&conn, "user_name", "TEXT")?;
    ensure_column(&conn, "user_username", "TEXT")?;
    ensure_column(&conn, "chat_title", "TEXT")?;
    ensure_column(&conn, "chat_username", "TEXT")?;
    ensure_column(&conn, "log_chat_id", "INTEGER")?;
    ensure_column(&conn, "log_message_thread_id", "INTEGER")?;
    ensure_column(&conn, "log_message_id", "INTEGER")?;
    conn.execute_batch(
        "UPDATE ban_release_jobs
         SET user_name = COALESCE(user_name, '-')
         WHERE user_name IS NULL;",
    )?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS captcha_sessions (
            chat_id INTEGER NOT NULL,
            user_id INTEGER NOT NULL,
            code TEXT NOT NULL,
            captcha_message_id INTEGER NOT NULL,
            options_json TEXT NOT NULL,
            attempts_left INTEGER NOT NULL,
            attempts_total INTEGER NOT NULL,
            expires_at INTEGER NOT NULL,
            user_first_name TEXT NOT NULL,
            user_last_name TEXT,
            user_username TEXT,
            chat_title TEXT,
            chat_username TEXT,
            PRIMARY KEY (chat_id, user_id)
        );
        CREATE INDEX IF NOT EXISTS idx_captcha_sessions_expires_at
            ON captcha_sessions (expires_at);",
    )?;
    Ok(())
}

fn ban_release_job_from_row(row: &rusqlite::Row<'_>) -> Result<BanReleaseJob, rusqlite::Error> {
    Ok(BanReleaseJob {
        chat_id: row.get(0)?,
        user_id: row.get(1)?,
        release_at: row.get(2)?,
        user_name: row.get(3)?,
        user_username: row.get(4)?,
        chat_title: row.get(5)?,
        chat_username: row.get(6)?,
        log_chat_id: row.get(7)?,
        log_message_thread_id: row.get(8)?,
        log_message_id: row.get(9)?,
    })
}

fn ensure_column(
    conn: &Connection,
    column_name: &str,
    column_definition: &str,
) -> Result<(), rusqlite::Error> {
    let mut stmt = conn.prepare("PRAGMA table_info(ban_release_jobs)")?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let existing_name: String = row.get(1)?;
        if existing_name == column_name {
            return Ok(());
        }
    }
    conn.execute(
        &format!("ALTER TABLE ban_release_jobs ADD COLUMN {column_name} {column_definition}"),
        [],
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BanReleaseJob {
    pub chat_id: i64,
    pub user_id: i64,
    pub release_at: i64,
    pub user_name: String,
    pub user_username: Option<String>,
    pub chat_title: Option<String>,
    pub chat_username: Option<String>,
    pub log_chat_id: Option<i64>,
    pub log_message_thread_id: Option<i32>,
    pub log_message_id: Option<i32>,
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temporary_db_path(label: &str) -> String {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        std::env::temp_dir()
            .join(format!(
                "telegram-buktikanbot-{label}-{}-{suffix}-{}",
                std::process::id(),
                NEXT_ID.fetch_add(1, Ordering::Relaxed)
            ))
            .to_string_lossy()
            .into_owned()
    }

    fn remove_database(path: &str) {
        let _ = fs::remove_file(path);
        let _ = fs::remove_file(format!("{path}-wal"));
        let _ = fs::remove_file(format!("{path}-shm"));
    }

    #[tokio::test]
    async fn ban_release_job_round_trip_and_delete() {
        let path = temporary_db_path("job");
        let store = BanReleaseStore::init(path.clone()).await.unwrap();
        let job = BanReleaseJob {
            chat_id: -100,
            user_id: 42,
            release_at: 100,
            user_name: "User".to_string(),
            user_username: Some("user".to_string()),
            chat_title: Some("Group".to_string()),
            chat_username: Some("group".to_string()),
            log_chat_id: Some(-200),
            log_message_thread_id: Some(42),
            log_message_id: Some(7),
        };

        store.upsert_job(job.clone()).await.unwrap();
        assert_eq!(store.fetch_due(99).await.unwrap(), Vec::new());
        assert_eq!(store.fetch_due(100).await.unwrap(), vec![job.clone()]);
        assert_eq!(store.count_pending().await.unwrap(), 1);
        assert_eq!(store.fetch_pending(0, 10).await.unwrap(), vec![job.clone()]);
        assert_eq!(store.get_job(-100, 42).await.unwrap(), Some(job.clone()));
        assert!(store.delete_job(job.chat_id, job.user_id).await.unwrap());
        assert!(store.fetch_due(100).await.unwrap().is_empty());
        assert!(!store.delete_job(job.chat_id, job.user_id).await.unwrap());
        assert_eq!(store.get_job(-100, 42).await.unwrap(), None);
        remove_database(&path);
    }

    #[tokio::test]
    async fn log_message_reference_can_be_attached_to_job() {
        let path = temporary_db_path("log-reference");
        let store = BanReleaseStore::init(path.clone()).await.unwrap();
        let job = BanReleaseJob {
            chat_id: -100,
            user_id: 42,
            release_at: 100,
            user_name: "User".to_string(),
            user_username: None,
            chat_title: None,
            chat_username: None,
            log_chat_id: Some(-200),
            log_message_thread_id: Some(42),
            log_message_id: None,
        };

        store.upsert_job(job).await.unwrap();
        store
            .attach_log_message(-100, 42, -200, Some(42), 7)
            .await
            .unwrap();
        let due = store.fetch_due(100).await.unwrap();
        assert_eq!(due[0].log_chat_id, Some(-200));
        assert_eq!(due[0].log_message_thread_id, Some(42));
        assert_eq!(due[0].log_message_id, Some(7));
        remove_database(&path);
    }

    #[tokio::test]
    async fn captcha_session_round_trip_and_delete() {
        let path = temporary_db_path("captcha");
        let store = BanReleaseStore::init(path.clone()).await.unwrap();
        let session = CaptchaSession {
            chat_id: -100,
            user_id: 42,
            code: "ABC123".to_string(),
            captcha_message_id: 7,
            options: vec!["ABC123".to_string(), "ZZZ999".to_string()],
            attempts_left: 2,
            attempts_total: 3,
            expires_at: 200,
            user_first_name: "User".to_string(),
            user_last_name: Some("Example".to_string()),
            user_username: Some("user".to_string()),
            chat_title: Some("Group".to_string()),
            chat_username: Some("group".to_string()),
        };

        store.save_captcha_session(session.clone()).await.unwrap();
        assert_eq!(
            store.fetch_captcha_sessions().await.unwrap(),
            vec![session.clone()]
        );
        store
            .delete_captcha_session(session.chat_id, session.user_id)
            .await
            .unwrap();
        assert!(store.fetch_captcha_sessions().await.unwrap().is_empty());
        remove_database(&path);
    }

    #[tokio::test]
    async fn legacy_ban_release_schema_is_migrated() {
        let path = temporary_db_path("migration");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE ban_release_jobs (
                chat_id INTEGER NOT NULL,
                user_id INTEGER NOT NULL,
                release_at INTEGER NOT NULL,
                PRIMARY KEY (chat_id, user_id)
            );",
        )
        .unwrap();
        drop(conn);

        let store = BanReleaseStore::init(path.clone()).await.unwrap();
        let job = BanReleaseJob {
            chat_id: -100,
            user_id: 42,
            release_at: 100,
            user_name: "User".to_string(),
            user_username: None,
            chat_title: None,
            chat_username: None,
            log_chat_id: None,
            log_message_thread_id: None,
            log_message_id: None,
        };
        store.upsert_job(job.clone()).await.unwrap();
        assert_eq!(store.fetch_due(100).await.unwrap(), vec![job]);
        remove_database(&path);
    }
}
