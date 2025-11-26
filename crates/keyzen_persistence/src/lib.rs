use anyhow::Result;
use keyzen_core::SessionStats;
use rusqlite::{params, Connection};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PersistenceError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Data not found")]
    NotFound,
}

/// 数据库管理器
pub struct Database {
    conn: Connection,
}

impl Database {
    /// 创建或打开数据库
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self> {
        let conn = Connection::open(path)?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    /// 使用默认路径创建数据库
    pub fn default() -> Result<Self> {
        let data_dir = Self::get_data_dir()?;
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join("keyzen.db");
        Self::new(db_path)
    }

    /// 获取数据目录路径
    fn get_data_dir() -> Result<PathBuf> {
        let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE"))?;
        Ok(PathBuf::from(home).join(".keyzen").join("data"))
    }

    /// 初始化数据库表
    fn initialize(&self) -> Result<()> {
        // 练习会话表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS sessions (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                lesson_id INTEGER NOT NULL,
                lesson_title TEXT NOT NULL,
                wpm REAL NOT NULL,
                cpm REAL NOT NULL,
                accuracy REAL NOT NULL,
                total_keystrokes INTEGER NOT NULL,
                error_count INTEGER NOT NULL,
                duration_secs INTEGER NOT NULL,
                completed_at INTEGER NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;

        // 薄弱按键表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS weak_keys (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                key_char TEXT NOT NULL,
                error_rate REAL NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 创建索引
        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_lesson_id ON sessions(lesson_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_sessions_completed_at ON sessions(completed_at)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_weak_keys_session_id ON weak_keys(session_id)",
            [],
        )?;

        Ok(())
    }

    /// 保存练习会话
    pub fn save_session(&self, stats: &SessionStats, lesson_title: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions (
                lesson_id, lesson_title, wpm, cpm, accuracy,
                total_keystrokes, error_count, duration_secs, completed_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                stats.lesson_id,
                lesson_title,
                stats.wpm,
                stats.cpm,
                stats.accuracy,
                stats.total_keystrokes,
                stats.error_count,
                stats.duration.as_secs() as i64,
                stats.timestamp,
            ],
        )?;

        let session_id = self.conn.last_insert_rowid();

        // 保存薄弱按键
        for (key_char, error_rate) in &stats.weak_keys {
            self.conn.execute(
                "INSERT INTO weak_keys (session_id, key_char, error_rate) VALUES (?1, ?2, ?3)",
                params![session_id, key_char.to_string(), error_rate],
            )?;
        }

        Ok(session_id)
    }

    /// 获取最近的练习记录
    pub fn get_recent_sessions(&self, limit: usize) -> Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, lesson_id, lesson_title, wpm, cpm, accuracy,
                    total_keystrokes, error_count, duration_secs, completed_at
             FROM sessions
             ORDER BY completed_at DESC
             LIMIT ?1",
        )?;

        let sessions = stmt
            .query_map([limit], |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    lesson_id: row.get(1)?,
                    lesson_title: row.get(2)?,
                    wpm: row.get(3)?,
                    cpm: row.get(4)?,
                    accuracy: row.get(5)?,
                    total_keystrokes: row.get(6)?,
                    error_count: row.get(7)?,
                    duration_secs: row.get(8)?,
                    completed_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    /// 获取指定课程的练习历史
    pub fn get_lesson_history(&self, lesson_id: i32, limit: usize) -> Result<Vec<SessionRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, lesson_id, lesson_title, wpm, cpm, accuracy,
                    total_keystrokes, error_count, duration_secs, completed_at
             FROM sessions
             WHERE lesson_id = ?1
             ORDER BY completed_at DESC
             LIMIT ?2",
        )?;

        let sessions = stmt
            .query_map(params![lesson_id, limit], |row| {
                Ok(SessionRecord {
                    id: row.get(0)?,
                    lesson_id: row.get(1)?,
                    lesson_title: row.get(2)?,
                    wpm: row.get(3)?,
                    cpm: row.get(4)?,
                    accuracy: row.get(5)?,
                    total_keystrokes: row.get(6)?,
                    error_count: row.get(7)?,
                    duration_secs: row.get(8)?,
                    completed_at: row.get(9)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(sessions)
    }

    /// 获取会话的薄弱按键
    pub fn get_weak_keys(&self, session_id: i64) -> Result<Vec<WeakKey>> {
        let mut stmt = self.conn.prepare(
            "SELECT key_char, error_rate FROM weak_keys WHERE session_id = ?1 ORDER BY error_rate DESC",
        )?;

        let keys = stmt
            .query_map([session_id], |row| {
                Ok(WeakKey {
                    key_char: row.get::<_, String>(0)?.chars().next().unwrap_or(' '),
                    error_rate: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(keys)
    }

    /// 获取所有会话的薄弱按键汇总（按平均错误率排序）
    pub fn get_overall_weak_keys(&self, limit: usize) -> Result<Vec<WeakKey>> {
        let mut stmt = self.conn.prepare(
            "SELECT key_char, AVG(error_rate) as avg_error_rate
             FROM weak_keys
             GROUP BY key_char
             ORDER BY avg_error_rate DESC
             LIMIT ?1",
        )?;

        let keys = stmt
            .query_map([limit], |row| {
                Ok(WeakKey {
                    key_char: row.get::<_, String>(0)?.chars().next().unwrap_or(' '),
                    error_rate: row.get(1)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(keys)
    }

    /// 获取所有时间的统计数据
    pub fn get_overall_stats(&self) -> Result<OverallStats> {
        let mut stmt = self.conn.prepare(
            "SELECT
                COUNT(*) as total_sessions,
                SUM(total_keystrokes) as total_keystrokes,
                AVG(wpm) as avg_wpm,
                MAX(wpm) as max_wpm,
                AVG(accuracy) as avg_accuracy
             FROM sessions",
        )?;

        let stats = stmt.query_row([], |row| {
            Ok(OverallStats {
                total_sessions: row.get(0)?,
                total_keystrokes: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
                avg_wpm: row.get::<_, Option<f64>>(2)?.unwrap_or(0.0),
                max_wpm: row.get::<_, Option<f64>>(3)?.unwrap_or(0.0),
                avg_accuracy: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
            })
        })?;

        Ok(stats)
    }
}

/// 会话记录
#[derive(Debug, Clone)]
pub struct SessionRecord {
    pub id: i64,
    pub lesson_id: i32,
    pub lesson_title: String,
    pub wpm: f64,
    pub cpm: f64,
    pub accuracy: f64,
    pub total_keystrokes: usize,
    pub error_count: usize,
    pub duration_secs: i64,
    pub completed_at: i64,
}

/// 薄弱按键
#[derive(Debug, Clone)]
pub struct WeakKey {
    pub key_char: char,
    pub error_rate: f32,
}

/// 总体统计
#[derive(Debug, Clone)]
pub struct OverallStats {
    pub total_sessions: i64,
    pub total_keystrokes: i64,
    pub avg_wpm: f64,
    pub max_wpm: f64,
    pub avg_accuracy: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::time::Duration;

    #[test]
    fn test_database_creation() {
        let db = Database::new(":memory:").unwrap();
        let stats = db.get_overall_stats().unwrap();
        assert_eq!(stats.total_sessions, 0);
    }

    #[test]
    fn test_save_and_retrieve_session() {
        let db = Database::new(":memory:").unwrap();

        let stats = SessionStats {
            lesson_id: 1,
            wpm: 45.5,
            cpm: 227.5,
            accuracy: 0.95,
            total_keystrokes: 100,
            error_count: 5,
            duration: Duration::from_secs(60),
            timestamp: Utc::now().timestamp(),
            weak_keys: vec![('a', 0.3), ('s', 0.2)],
        };

        let session_id = db.save_session(&stats, "Test Lesson").unwrap();
        assert!(session_id > 0);

        let sessions = db.get_recent_sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].lesson_title, "Test Lesson");
        assert_eq!(sessions[0].wpm, 45.5);

        let weak_keys = db.get_weak_keys(session_id).unwrap();
        assert_eq!(weak_keys.len(), 2);
    }
}
