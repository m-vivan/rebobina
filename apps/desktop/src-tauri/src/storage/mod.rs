use crate::error::{AppError, AppResult};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

const KEYRING_SERVICE: &str = "dev.vivan.rebobina";
const LEGACY_KEYRING_SERVICE: &str = "com.vivan.gravador";
const KEYRING_USER: &str = "openai_api_key";
const APP_DATA_DIR_NAME: &str = "rebobina";
const LEGACY_DATA_DIR_NAME: &str = "gravador";
const DB_FILE_NAME: &str = "rebobina.db";
const LEGACY_DB_FILE_NAME: &str = "gravador.db";

const SCHEMA: &str = r#"
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
            "#;

fn apply_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(SCHEMA)?;
    ensure_column(conn, "meetings", "description", "TEXT")?;
    ensure_column(conn, "summaries", "details_json", "TEXT")?;
    Ok(())
}

/// Adiciona a coluna se o banco foi criado antes dela existir.
fn ensure_column(conn: &Connection, table: &str, column: &str, ty: &str) -> AppResult<()> {
    let mut stmt = conn.prepare(&format!("PRAGMA table_info({table})"))?;
    let exists = stmt
        .query_map([], |row| row.get::<_, String>(1))?
        .filter_map(Result::ok)
        .any(|name| name == column);
    if !exists {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ty};"))?;
    }
    Ok(())
}

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
    #[serde(default)]
    pub description: Option<String>,
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
    /// JSON completo do resumo estruturado (participantes, pontos-chave etc.).
    #[serde(default)]
    pub details_json: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub default_device_name: Option<String>,
    pub language: String,
    #[serde(default = "default_true")]
    pub auto_detect_calls: bool,
    /// Nomes, empresas e termos que costumam aparecer nas reuniões, um por
    /// linha ou separados por vírgula. Orientam a transcrição e o resumo.
    #[serde(default)]
    pub vocabulary: String,
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
        let data_dir = resolve_data_dir()?;
        fs::create_dir_all(data_dir.join("meetings"))?;

        let db_path = resolve_db_path(&data_dir)?;
        let conn = Connection::open(&db_path)?;
        apply_schema(&conn)?;

        rewrite_legacy_audio_paths(&conn, &data_dir)?;

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
            description: None,
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

    pub fn update_meeting_details(
        &self,
        meeting_id: &str,
        title: &str,
        description: Option<&str>,
    ) -> AppResult<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::Message("O nome da reunião não pode ficar vazio".into()));
        }
        let description = description.map(str::trim).filter(|d| !d.is_empty());
        let conn = self.conn.lock().unwrap();
        let n = conn.execute(
            "UPDATE meetings SET title = ?1, description = ?2 WHERE id = ?3",
            params![title, description, meeting_id],
        )?;
        if n == 0 {
            return Err(AppError::Message("Reunião não encontrada".into()));
        }
        Ok(())
    }

    pub fn set_title(&self, meeting_id: &str, title: &str) -> AppResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE meetings SET title = ?1 WHERE id = ?2",
            params![title, meeting_id],
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
            "SELECT id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name, description
             FROM meetings WHERE id = ?1",
        )?;
        let meeting = stmt.query_row(params![meeting_id], map_meeting).optional()?;
        Ok(meeting)
    }

    pub fn list_meetings(&self, query: Option<&str>) -> AppResult<Vec<Meeting>> {
        let conn = self.conn.lock().unwrap();
        let mut meetings = Vec::new();

        if let Some(q) = query.filter(|s| !s.trim().is_empty()) {
            let like = format!("%{}%", q.trim());
            let mut stmt = conn.prepare(
                "SELECT DISTINCT m.id, m.title, m.started_at, m.ended_at, m.duration_sec, m.audio_path, m.status, m.error_message, m.device_name, m.description
                 FROM meetings m
                 LEFT JOIN transcripts t ON t.meeting_id = m.id
                 WHERE m.title LIKE ?1 OR IFNULL(m.description, '') LIKE ?1 OR IFNULL(t.full_text, '') LIKE ?1
                 ORDER BY m.started_at DESC",
            )?;
            let rows = stmt.query_map(params![like], map_meeting)?;
            for row in rows {
                meetings.push(row?);
            }
        } else {
            let mut stmt = conn.prepare(
                "SELECT id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name, description
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
                notable_quotes_json, raw_markdown, model, created_at, details_json
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(meeting_id) DO UPDATE SET
                overview = excluded.overview,
                decisions_json = excluded.decisions_json,
                action_items_json = excluded.action_items_json,
                topics_json = excluded.topics_json,
                notable_quotes_json = excluded.notable_quotes_json,
                raw_markdown = excluded.raw_markdown,
                model = excluded.model,
                created_at = excluded.created_at,
                details_json = excluded.details_json",
            params![
                summary.meeting_id,
                summary.overview,
                summary.decisions_json,
                summary.action_items_json,
                summary.topics_json,
                summary.notable_quotes_json,
                summary.raw_markdown,
                summary.model,
                summary.created_at,
                summary.details_json
            ],
        )?;
        Ok(())
    }

    pub fn get_summary(&self, meeting_id: &str) -> AppResult<Option<Summary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT meeting_id, overview, decisions_json, action_items_json, topics_json,
                    notable_quotes_json, raw_markdown, model, created_at, details_json
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
                    details_json: row.get(9)?,
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
                eprintln!("rebobina: falha ao remover pasta da reunião: {e}");
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
        let vocabulary: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'vocabulary'",
                [],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_default();
        Ok(AppSettings {
            default_device_name,
            language,
            auto_detect_calls,
            vocabulary,
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
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('vocabulary', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![settings.vocabulary.trim()],
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
                Ok(()) => {}
                Err(keyring::Error::NoEntry) => {}
                Err(e) => return Err(e.into()),
            }
            // Limpa também a entrada legada, se existir.
            if let Ok(legacy) = keyring::Entry::new(LEGACY_KEYRING_SERVICE, KEYRING_USER) {
                let _ = legacy.delete_credential();
            }
            Ok(())
        } else {
            entry.set_password(api_key.trim())?;
            Ok(())
        }
    }

    pub fn get_api_key() -> AppResult<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => {
                // Migra API key do serviço antigo (com.vivan.gravador), se houver.
                let legacy = keyring::Entry::new(LEGACY_KEYRING_SERVICE, KEYRING_USER)?;
                match legacy.get_password() {
                    Ok(value) => {
                        if let Err(e) = entry.set_password(&value) {
                            eprintln!("rebobina: não migrou API key para {KEYRING_SERVICE}: {e}");
                        } else {
                            let _ = legacy.delete_credential();
                            eprintln!(
                                "rebobina: migrou API key de {LEGACY_KEYRING_SERVICE} → {KEYRING_SERVICE}"
                            );
                        }
                        Ok(Some(value))
                    }
                    Err(keyring::Error::NoEntry) => Ok(None),
                    Err(e) => Err(e.into()),
                }
            }
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
        description: row.get(9)?,
    })
}

/// Resolve `~/Library/Application Support/rebobina`, migrando de `gravador` se necessário.
fn resolve_data_dir() -> AppResult<PathBuf> {
    let base = dirs::data_dir()
        .ok_or_else(|| AppError::Message("Não foi possível resolver o diretório de dados".into()))?;
    migrate_legacy_data(&base)
}

/// Leva os dados de `base/gravador` para `base/rebobina`.
///
/// - Só a pasta antiga existe: renomeia (ou copia, entre volumes).
/// - As duas existem (ex.: o app instalado criou `rebobina/` e um build antigo
///   continuou gravando em `gravador/`): junta as reuniões da antiga na nova
///   e guarda a antiga como `gravador.migrado-<data>`, sem apagar nada.
///
/// Depois da migração a pasta antiga deixa de existir, então rodar de novo não faz nada.
fn migrate_legacy_data(base: &Path) -> AppResult<PathBuf> {
    let new_dir = base.join(APP_DATA_DIR_NAME);
    let old_dir = base.join(LEGACY_DATA_DIR_NAME);

    if !old_dir.exists() {
        return Ok(new_dir);
    }

    if !new_dir.exists() {
        match fs::rename(&old_dir, &new_dir) {
            Ok(()) => {
                eprintln!(
                    "rebobina: migrou dados de {:?} para {:?}",
                    old_dir, new_dir
                );
            }
            Err(rename_err) => {
                // Fallback (ex.: volumes cruzados): copia e remove a pasta antiga.
                copy_dir_recursive(&old_dir, &new_dir)?;
                if let Err(e) = fs::remove_dir_all(&old_dir) {
                    eprintln!(
                        "rebobina: dados copiados para {:?}, mas não foi possível remover {:?}: {e}",
                        new_dir, old_dir
                    );
                } else {
                    eprintln!(
                        "rebobina: migrou (cópia) dados de {:?} para {:?} (rename falhou: {rename_err})",
                        old_dir, new_dir
                    );
                }
            }
        }
        return Ok(new_dir);
    }

    merge_legacy_dir(&old_dir, &new_dir)?;

    let stamp = Utc::now().format("%Y%m%d-%H%M%S");
    let archived = base.join(format!("{LEGACY_DATA_DIR_NAME}.migrado-{stamp}"));
    fs::rename(&old_dir, &archived)?;
    eprintln!(
        "rebobina: juntou {:?} em {:?}; pasta antiga guardada em {:?}",
        old_dir, new_dir, archived
    );
    Ok(new_dir)
}

/// Importa para `new_dir` as reuniões de `old_dir` que ainda não existem lá:
/// linhas do banco (reuniões, transcrições, resumos e configurações ausentes)
/// e as pastas de áudio. O que já existe na pasta nova nunca é sobrescrito.
fn merge_legacy_dir(old_dir: &Path, new_dir: &Path) -> AppResult<()> {
    let old_db = [LEGACY_DB_FILE_NAME, DB_FILE_NAME]
        .iter()
        .map(|name| old_dir.join(name))
        .find(|p| p.exists());

    if let Some(old_db) = old_db {
        let conn = Connection::open(resolve_db_path(new_dir)?)?;
        apply_schema(&conn)?;
        let old_db = old_db
            .to_str()
            .ok_or_else(|| AppError::Message("Caminho do banco antigo inválido".into()))?;
        conn.execute("ATTACH DATABASE ?1 AS legacy", params![old_db])?;
        let imported = conn.execute_batch(
            r#"
            BEGIN;
            INSERT OR IGNORE INTO meetings
                (id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name)
                SELECT id, title, started_at, ended_at, duration_sec, audio_path, status, error_message, device_name
                FROM legacy.meetings;
            INSERT OR IGNORE INTO transcripts (meeting_id, full_text, segments_json)
                SELECT meeting_id, full_text, segments_json FROM legacy.transcripts
                WHERE meeting_id IN (SELECT id FROM meetings);
            INSERT OR IGNORE INTO summaries
                (meeting_id, overview, decisions_json, action_items_json, topics_json,
                 notable_quotes_json, raw_markdown, model, created_at)
                SELECT meeting_id, overview, decisions_json, action_items_json, topics_json,
                       notable_quotes_json, raw_markdown, model, created_at
                FROM legacy.summaries
                WHERE meeting_id IN (SELECT id FROM meetings);
            INSERT OR IGNORE INTO settings (key, value) SELECT key, value FROM legacy.settings;
            COMMIT;
            "#,
        );
        if let Err(e) = imported {
            let _ = conn.execute_batch("ROLLBACK;");
            return Err(e.into());
        }
        conn.execute("DETACH DATABASE legacy", [])?;
        rewrite_legacy_audio_paths(&conn, new_dir)?;
    }

    let old_meetings = old_dir.join("meetings");
    if old_meetings.exists() {
        let new_meetings = new_dir.join("meetings");
        fs::create_dir_all(&new_meetings)?;
        for entry in fs::read_dir(&old_meetings)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let target = new_meetings.join(entry.file_name());
            if target.exists() {
                continue;
            }
            if fs::rename(entry.path(), &target).is_err() {
                copy_dir_recursive(&entry.path(), &target)?;
            }
        }
    }

    Ok(())
}

fn resolve_db_path(data_dir: &Path) -> AppResult<PathBuf> {
    let new_db = data_dir.join(DB_FILE_NAME);
    let old_db = data_dir.join(LEGACY_DB_FILE_NAME);
    if !new_db.exists() && old_db.exists() {
        fs::rename(&old_db, &new_db)?;
        eprintln!(
            "rebobina: renomeou banco {:?} → {:?}",
            old_db.file_name(),
            new_db.file_name()
        );
    }
    Ok(new_db)
}

/// Atualiza `audio_path` absolutos que ainda apontam para a pasta legada `gravador`.
fn rewrite_legacy_audio_paths(conn: &Connection, data_dir: &Path) -> AppResult<()> {
    let old_marker = format!("/{LEGACY_DATA_DIR_NAME}/");
    let new_marker = format!("/{APP_DATA_DIR_NAME}/");
    let updated = conn.execute(
        "UPDATE meetings SET audio_path = REPLACE(audio_path, ?1, ?2) WHERE audio_path LIKE ?3",
        params![old_marker, new_marker, format!("%{old_marker}%")],
    )?;
    if updated > 0 {
        eprintln!(
            "rebobina: reescreveu {updated} audio_path(s) para {:?}",
            data_dir
        );
    }
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_base() -> PathBuf {
        let base = std::env::temp_dir().join(format!("rebobina-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&base).unwrap();
        base
    }

    /// Cria uma pasta de dados com um banco e uma reunião (com áudio e transcrição).
    fn seed(dir: &Path, db_name: &str, meeting_id: &str, title: &str) {
        let meeting_dir = dir.join("meetings").join(meeting_id);
        fs::create_dir_all(&meeting_dir).unwrap();
        let audio = meeting_dir.join("audio.wav");
        fs::write(&audio, b"RIFF").unwrap();

        let conn = Connection::open(dir.join(db_name)).unwrap();
        apply_schema(&conn).unwrap();
        conn.execute(
            "INSERT INTO meetings (id, title, started_at, audio_path, status) VALUES (?1, ?2, ?3, ?4, 'ready')",
            params![meeting_id, title, "2026-10-19T18:00:00Z", audio.to_str().unwrap()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO transcripts (meeting_id, full_text, segments_json) VALUES (?1, ?2, '[]')",
            params![meeting_id, format!("texto de {title}")],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('language', ?1)",
            params![if db_name == LEGACY_DB_FILE_NAME { "en" } else { "pt" }],
        )
        .unwrap();
    }

    fn archived_dirs(base: &Path) -> Vec<PathBuf> {
        fs::read_dir(base)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("gravador.migrado-"))
            })
            .collect()
    }

    #[test]
    fn renames_legacy_dir_when_new_one_is_missing() {
        let base = temp_base();
        seed(&base.join("gravador"), LEGACY_DB_FILE_NAME, "a", "Antiga");

        let dir = migrate_legacy_data(&base).unwrap();

        assert_eq!(dir, base.join("rebobina"));
        assert!(!base.join("gravador").exists());
        assert!(dir.join("meetings/a/audio.wav").exists());
        assert!(archived_dirs(&base).is_empty());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn merges_legacy_dir_into_existing_new_dir() {
        let base = temp_base();
        let old_dir = base.join("gravador");
        let new_dir = base.join("rebobina");
        seed(&old_dir, LEGACY_DB_FILE_NAME, "a", "Antiga");
        seed(&new_dir, DB_FILE_NAME, "b", "Nova");

        let dir = migrate_legacy_data(&base).unwrap();
        assert_eq!(dir, new_dir);

        let conn = Connection::open(new_dir.join(DB_FILE_NAME)).unwrap();
        let mut stmt = conn
            .prepare("SELECT m.id, m.audio_path, t.full_text FROM meetings m JOIN transcripts t ON t.meeting_id = m.id ORDER BY m.id")
            .unwrap();
        let rows: Vec<(String, String, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "a");
        assert_eq!(rows[0].2, "texto de Antiga");
        assert!(rows[0].1.contains("/rebobina/meetings/a/"), "{}", rows[0].1);
        assert!(new_dir.join("meetings/a/audio.wav").exists());

        // A configuração que já existia na pasta nova é mantida.
        let language: String = conn
            .query_row("SELECT value FROM settings WHERE key = 'language'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(language, "pt");

        // A pasta antiga não é apagada: fica guardada com outro nome.
        assert!(!old_dir.exists());
        let archived = archived_dirs(&base);
        assert_eq!(archived.len(), 1);
        assert!(archived[0].join(LEGACY_DB_FILE_NAME).exists());
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn merge_is_a_no_op_on_second_run() {
        let base = temp_base();
        seed(&base.join("gravador"), LEGACY_DB_FILE_NAME, "a", "Antiga");
        seed(&base.join("rebobina"), DB_FILE_NAME, "b", "Nova");

        migrate_legacy_data(&base).unwrap();
        migrate_legacy_data(&base).unwrap();

        let conn = Connection::open(base.join("rebobina").join(DB_FILE_NAME)).unwrap();
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM meetings", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
        assert_eq!(archived_dirs(&base).len(), 1);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn never_overwrites_meeting_that_exists_in_both() {
        let base = temp_base();
        seed(&base.join("gravador"), LEGACY_DB_FILE_NAME, "x", "Versão antiga");
        seed(&base.join("rebobina"), DB_FILE_NAME, "x", "Versão nova");

        migrate_legacy_data(&base).unwrap();

        let conn = Connection::open(base.join("rebobina").join(DB_FILE_NAME)).unwrap();
        let title: String = conn
            .query_row("SELECT title FROM meetings WHERE id = 'x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(title, "Versão nova");
        let _ = fs::remove_dir_all(base);
    }
}
