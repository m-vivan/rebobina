use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

const KEYRING_SERVICE: &str = "com.vivan.gravador";
const KEYRING_USER: &str = "openai_api_key";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub duration_sec: Option<i64>,
    pub audio_path: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub device_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transcript {
    pub meeting_id: String,
    pub full_text: String,
    pub segments_json: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub meeting_id: String,
    pub overview: String,
    pub decisions_json: String,
    pub action_items_json: String,
    pub topics_json: String,
    pub notable_quotes_json: String,
    pub raw_markdown: String,
    pub model: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_device_name: Option<String>,
    pub language: String,
    #[serde(default = "default_true")]
    pub auto_detect_calls: bool,
}

fn default_true() -> bool {
    true
}

pub struct Storage {
    conn: Mutex<Connection>,
    data_dir: PathBuf,
}

impl Storage {
    pub fn init() -> AppResult<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| AppError::Message("Não foi possível resolver o diretório de dados".into()))?
            .join("gravador");
        fs::create_dir_all(data_dir.join("meetings"))?;

        let db_path = data_dir.join("gravador.db");
        let conn = Connection::open(&db_path)?;
        conn.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS meetings (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                started_at TEXT NOT NULL,
                ended_at TEXT,
                duration_sec INTEGER,
                audio_path TEXT,
                status TEXT NOT NULL,
                error_message TEXT,
                device_name TEXT
            );
            CREATE TABLE IF NOT EXISTS transcripts (
                meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
                full_text TEXT NOT NULL,
                segments_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS summaries (
                meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
                overview TEXT NOT NULL,
                decisions_json TEXT NOT NULL,
                action_items_json TEXT NOT NULL,
                topics_json TEXT NOT NULL,
                notable_quotes_json TEXT NOT NULL,
                raw_markdown TEXT NOT NULL,
                model TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
            data_dir,
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    pub fn meeting_dir(&self, meeting_id: &str) -> PathBuf {
        self.data_dir.join("meetings").join(meeting_id)
    }

    pub fn create_meeting(&self, title: &str, device_name: Option<&str>) -> AppResult<Meeting> {
        let id = Uuid::new_v4().to_string();
        let started_at = Utc::now().to_rfc3339();
        let dir = self.meeting_dir(&id);
        fs::create_dir_all(&dir)?;

        let meeting = Meeting {
            id: id.clone(),
            title: title.to_string(),
            started_at: started_at.clone(),
            ended_at: None,
            duration_sec: None,
            audio_path: None,
            status: "recording".into(),
            error_message: None,
            device_name: device_name.map(|s| s.to_string()),
        };

        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO meetings (id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name)
             VALUES (?1, ?2, ?3, NULL, NULL, NULL, ?4, NULL, ?5)",
            params![
                meeting.id,
                meeting.title,
                meeting.started_at,
                meeting.status,
                meeting.device_name
            ],
        )?;
        Ok(meeting)
    }

    pub fn finish_recording(
        &self,
        meeting_id: &str,
        audio_path: &Path,
        ended_at: DateTime<Utc>,
        duration_sec: i64,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE meetings SET ended_at = ?1, duration_sec = ?2, audio_path = ?3, status = 'processing', error_message = NULL WHERE id = ?4",
            params![
                ended_at.to_rfc3339(),
                duration_sec,
                audio_path.to_string_lossy().to_string(),
                meeting_id
            ],
        )?;
        Ok(())
    }

    pub fn set_status(
        &self,
        meeting_id: &str,
        status: &str,
        error_message: Option<&str>,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE meetings SET status = ?1, error_message = ?2 WHERE id = ?3",
            params![status, error_message, meeting_id],
        )?;
        Ok(())
    }

    pub fn get_meeting(&self, meeting_id: &str) -> AppResult<Option<Meeting>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name
             FROM meetings WHERE id = ?1",
        )?;
        let meeting = stmt
            .query_row(params![meeting_id], |row| {
                Ok(Meeting {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    started_at: row.get(2)?,
                    ended_at: row.get(3)?,
                    duration_sec: row.get(4)?,
                    audio_path: row.get(5)?,
                    status: row.get(6)?,
                    error_message: row.get(7)?,
                    device_name: row.get(8)?,
                })
            })
            .optional()?;
        Ok(meeting)
    }

    pub fn list_meetings(&self, query: Option<&str>) -> AppResult<Vec<Meeting>> {
        let conn = self.conn.lock().unwrap();
        let mut meetings = Vec::new();

        if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
            let like = format!("%{}%", q.trim());
            let mut stmt = conn.prepare(
                "SELECT DISTINCT m.id, m.title, m.started_at, m.ended_at, m.duration_sec, m.audio_path, m.status, m.error_message, m.device_name
                 FROM meetings m
                 LEFT JOIN transcripts t ON t.meeting_id = m.id
                 WHERE m.title LIKE ?1 OR IFNULL(t.full_text, '') LIKE ?1
                 ORDER BY m.started_at DESC",
            )?;
            let rows = stmt.query_map(params![like], map_meeting)?;
            for row in rows {
                meetings.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name
                 FROM meetings ORDER BY started_at DESC",
            )?;
            let rows = stmt.query_map([], map_meeting)?;
            for row in rows {
                meetings.push(row?);
            }
        }
        Ok(meetings)
    }

    pub fn save_transcript(
        &self,
        meeting_id: &str,
        full_text: &str,
        segments_json: &str,
    ) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO transcripts (meeting_id, full_text, segments_json)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(meeting_id) DO UPDATE SET full_text = excluded.full_text, segments_json = excluded.segments_json",
            params![meeting_id, full_text, segments_json],
        )?;
        Ok(())
    }

    pub fn get_transcript(&self, meeting_id: &str) -> AppResult<Option<Transcript>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT meeting_id, full_text, segments_json FROM transcripts WHERE meeting_id = ?1",
        )?;
        let result = stmt
            .query_row(params![meeting_id], |row| {
                Ok(Transcript {
                    meeting_id: row.get(0)?,
                    full_text: row.get(1)?,
                    segments_json: row.get(2)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    pub fn save_summary(&self, summary: &Summary) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO summaries (
                meeting_id, overview, decisions_json, action_items_json, topics_json,
                notable_quotes_json, raw_markdown, model, created_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(meeting_id) DO UPDATE SET
                overview = excluded.overview,
                decisions_json = excluded.decisions_json,
                action_items_json = excluded.action_items_json,
                topics_json = excluded.topics_json,
                notable_quotes_json = excluded.notable_quotes_json,
                raw_markdown = excluded.raw_markdown,
                model = excluded.model,
                created_at = excluded.created_at",
            params![
                summary.meeting_id,
                summary.overview,
                summary.decisions_json,
                summary.action_items_json,
                summary.topics_json,
                summary.notable_quotes_json,
                summary.raw_markdown,
                summary.model,
                summary.created_at
            ],
        )?;
        Ok(())
    }

    pub fn get_summary(&self, meeting_id: &str) -> AppResult<Option<Summary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT meeting_id, overview, decisions_json, action_items_json, topics_json,
                    notable_quotes_json, raw_markdown, model, created_at
             FROM summaries WHERE meeting_id = ?1",
        )?;
        let result = stmt
            .query_row(params![meeting_id], |row| {
                Ok(Summary {
                    meeting_id: row.get(0)?,
                    overview: row.get(1)?,
                    decisions_json: row.get(2)?,
                    action_items_json: row.get(3)?,
                    topics_json: row.get(4)?,
                    notable_quotes_json: row.get(5)?,
                    raw_markdown: row.get(6)?,
                    model: row.get(7)?,
                    created_at: row.get(8)?,
                })
            })
            .optional()?;
        Ok(result)
    }

    pub fn delete_meeting(&self, meeting_id: &str) -> AppResult<()> {
        {
            let conn = self.conn.lock().unwrap();
            // Garante CASCADE mesmo se a conexão foi reaberta sem o pragma.
            conn.execute_batch("PRAGMA foreign_keys = ON;")?;
            // Remove filhos explicitamente (DBs antigos podem não ter ON DELETE CASCADE).
            conn.execute(
                "DELETE FROM transcripts WHERE meeting_id = ?1",
                params![meeting_id],
            )?;
            conn.execute(
                "DELETE FROM summaries WHERE meeting_id = ?1",
                params![meeting_id],
            )?;
            let n = conn.execute("DELETE FROM meetings WHERE id = ?1", params![meeting_id])?;
            if n == 0 {
                return Err(AppError::Message("Reunião não encontrada".into()));
            }
        }
        let dir = self.meeting_dir(meeting_id);
        if dir.exists() {
            if let Err(e) = fs::remove_dir_all(&dir) {
                // DB já apagada; pasta residual não deve impedir o fluxo.
                eprintln!("gravador: falha ao remover pasta da reunião: {e}");
            }
        }
        Ok(())
    }

    pub fn get_settings(&self) -> AppResult<AppSettings> {
        let conn = self.conn.lock().unwrap();
        let default_device_name: Option<String> = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'default_device_name'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let language: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'language'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "pt".into());
        let auto_detect_calls: bool = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'auto_detect_calls'",
                [],
                |row| row.get::<_, String>(0),
            )
            .optional()?
            .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
            .unwrap_or(true);
        Ok(AppSettings {
            default_device_name,
            language,
            auto_detect_calls,
        })
    }

    pub fn save_settings(&self, settings: &AppSettings) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('language', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![settings.language],
        )?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('auto_detect_calls', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![if settings.auto_detect_calls {
                "1"
            } else {
                "0"
            }],
        )?;
        match &settings.default_device_name {
            Some(name) => {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('default_device_name', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![name],
                )?;
            }
            None => {
                conn.execute(
                    "DELETE FROM settings WHERE key = 'default_device_name'",
                    [],
                )?;
            }
        }
        Ok(())
    }

    pub fn set_api_key(api_key: &str) -> AppResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        if api_key.trim().is_empty() {
            match entry.delete_credential() {
                Ok(()) => Ok(()),
                Err(keyring::Error::NoEntry) => Ok(()),
                Err(e) => Err(e.into()),
            }
        } else {
            entry.set_password(api_key.trim())?;
            Ok(())
        }
    }

    pub fn get_api_key() -> AppResult<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn has_api_key() -> AppResult<bool> {
        Ok(Self::get_api_key()?.is_some())
    }
}

fn map_meeting(row: &rusqlite::Row<'_>) -> rusqlite::Result<Meeting> {
    Ok(Meeting {
        id: row.get(0)?,
        title: row.get(1)?,
        started_at: row.get(2)?,
        ended_at: row.get(3)?,
        duration_sec: row.get(4)?,
        audio_path: row.get(5)?,
        status: row.get(6)?,
        error_message: row.get(7)?,
        device_name: row.get(8)?,
    })
}
