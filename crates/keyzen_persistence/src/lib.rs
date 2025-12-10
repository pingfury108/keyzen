use anyhow::Result;
use keyzen_core::{SessionStats, UnitType, WeakUnit};
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

        // 启用外键约束
        conn.execute("PRAGMA foreign_keys = ON", [])?;

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

        // 薄弱单元表（新表结构）
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS weak_units (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id INTEGER NOT NULL,
                content TEXT NOT NULL,
                unit_type TEXT NOT NULL,
                error_count INTEGER NOT NULL,
                total_count INTEGER NOT NULL,
                error_rate REAL NOT NULL,
                FOREIGN KEY(session_id) REFERENCES sessions(id) ON DELETE CASCADE
            )",
            [],
        )?;

        // 配置表
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS config (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
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
            "CREATE INDEX IF NOT EXISTS idx_weak_units_session_id ON weak_units(session_id)",
            [],
        )?;

        self.conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_weak_units_error_rate ON weak_units(error_rate DESC)",
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
                stats.overall_wpm,
                stats.overall_cpm,
                stats.overall_accuracy,
                stats.total_keystrokes,
                stats.error_count,
                stats.duration_secs as i64,
                stats.timestamp,
            ],
        )?;

        let session_id = self.conn.last_insert_rowid();

        // 保存薄弱单元
        self.save_weak_units(session_id, &stats.weak_units)?;

        Ok(session_id)
    }

    /// 保存薄弱单元
    pub fn save_weak_units(&self, session_id: i64, units: &[WeakUnit]) -> Result<()> {
        let mut stmt = self.conn.prepare(
            "INSERT INTO weak_units (session_id, content, unit_type, error_count, total_count, error_rate)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)"
        )?;

        for unit in units {
            stmt.execute(params![
                session_id,
                &unit.content,
                unit.unit_type.as_str(),
                unit.error_count,
                unit.total_count,
                unit.error_rate,
            ])?;
        }

        Ok(())
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

    /// 获取会话的薄弱单元
    pub fn get_weak_units(&self, session_id: i64) -> Result<Vec<WeakUnit>> {
        let mut stmt = self.conn.prepare(
            "SELECT content, unit_type, error_count, total_count, error_rate
             FROM weak_units
             WHERE session_id = ?1
             ORDER BY error_rate DESC",
        )?;

        let units = stmt
            .query_map([session_id], |row| {
                let unit_type_str: String = row.get(1)?;
                Ok(WeakUnit {
                    content: row.get(0)?,
                    unit_type: UnitType::from_str(&unit_type_str),
                    error_count: row.get(2)?,
                    total_count: row.get(3)?,
                    error_rate: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(units)
    }

    /// 获取所有会话的薄弱单元汇总（按错误率排序）
    pub fn get_overall_weak_units(&self, limit: usize) -> Result<Vec<WeakUnit>> {
        let mut stmt = self.conn.prepare(
            "SELECT content, unit_type,
                    SUM(error_count) as total_errors,
                    SUM(total_count) as total_occurrences,
                    CAST(SUM(error_count) AS REAL) / CAST(SUM(total_count) AS REAL) as avg_error_rate
             FROM weak_units
             GROUP BY content, unit_type
             HAVING total_occurrences >= 1 AND avg_error_rate > 0.10
             ORDER BY avg_error_rate DESC
             LIMIT ?1",
        )?;

        let units = stmt
            .query_map([limit], |row| {
                let unit_type_str: String = row.get(1)?;
                Ok(WeakUnit {
                    content: row.get(0)?,
                    unit_type: UnitType::from_str(&unit_type_str),
                    error_count: row.get(2)?,
                    total_count: row.get(3)?,
                    error_rate: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(units)
    }

    /// 获取会话的薄弱按键（兼容旧 API）
    #[deprecated(note = "使用 get_weak_units 代替")]
    pub fn get_weak_keys(&self, session_id: i64) -> Result<Vec<WeakKey>> {
        let units = self.get_weak_units(session_id)?;
        Ok(units
            .into_iter()
            .filter_map(|unit| {
                unit.content.chars().next().map(|ch| WeakKey {
                    key_char: ch,
                    error_rate: unit.error_rate,
                })
            })
            .collect())
    }

    /// 获取所有会话的薄弱按键汇总（兼容旧 API）
    #[deprecated(note = "使用 get_overall_weak_units 代替")]
    pub fn get_overall_weak_keys(&self, limit: usize) -> Result<Vec<WeakKey>> {
        let units = self.get_overall_weak_units(limit)?;
        Ok(units
            .into_iter()
            .filter_map(|unit| {
                unit.content.chars().next().map(|ch| WeakKey {
                    key_char: ch,
                    error_rate: unit.error_rate,
                })
            })
            .collect())
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

impl Database {
    /// 保存配置项
    pub fn save_config(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO config (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    /// 获取配置项
    pub fn get_config(&self, key: &str) -> Result<Option<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT value FROM config WHERE key = ?1")?;
        let mut rows = stmt.query(params![key])?;

        if let Some(row) = rows.next()? {
            Ok(Some(row.get(0)?))
        } else {
            Ok(None)
        }
    }

    /// 删除配置项
    pub fn delete_config(&self, key: &str) -> Result<()> {
        self.conn
            .execute("DELETE FROM config WHERE key = ?1", params![key])?;
        Ok(())
    }
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
            weak_units: vec![
                WeakUnit {
                    content: "a".to_string(),
                    unit_type: UnitType::Character,
                    error_count: 3,
                    total_count: 10,
                    error_rate: 0.3,
                },
                WeakUnit {
                    content: "s".to_string(),
                    unit_type: UnitType::Character,
                    error_count: 2,
                    total_count: 10,
                    error_rate: 0.2,
                },
            ],
        };

        let session_id = db.save_session(&stats, "Test Lesson").unwrap();
        assert!(session_id > 0);

        let sessions = db.get_recent_sessions(10).unwrap();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].lesson_title, "Test Lesson");
        assert_eq!(sessions[0].wpm, 45.5);

        let weak_units = db.get_weak_units(session_id).unwrap();
        assert_eq!(weak_units.len(), 2);
    }
}
