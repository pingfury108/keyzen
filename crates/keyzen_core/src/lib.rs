use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 课程类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LessonType {
    Prose,        // 散文
    Code,         // 代码
    SpecialChars, // 特殊符号
    Pinyin,       // 拼音练习
}

/// 难度等级
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    Beginner,
    Intermediate,
    Advanced,
}

/// 课程元数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonMeta {
    pub difficulty: Difficulty,
    pub tags: Vec<String>,
    #[serde(with = "duration_serde")]
    pub estimated_time: Duration,
    pub prerequisite_ids: Vec<u32>,
}

/// 课程定义
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: u32,
    pub lesson_type: LessonType,
    pub language: String,
    pub title: String,
    pub description: String,
    pub source_text: String,
    pub meta: LessonMeta,
}

/// 输入模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum InputMode {
    Strict,      // 必须纠正错误
    Forgiving,   // 标记错误但可继续（默认）
    Invisible,   // 不显示错误（盲打）
}

impl Default for InputMode {
    fn default() -> Self {
        Self::Forgiving
    }
}

/// 练习模式
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum PracticeMode {
    Zen,     // 禅意模式（无时间压力）
    Timed,   // 限时挑战
    Endless, // 无限模式
}

impl Default for PracticeMode {
    fn default() -> Self {
        Self::Zen
    }
}

/// 打字事件（用于 UI 反馈）
#[derive(Debug, Clone)]
pub enum TypingEvent {
    KeyPressed {
        char: char,
        correct: bool,
        position: usize,
    },
    WordCompleted {
        wpm: f64,
    },
    MilestoneReached {
        progress: f32, // 0.0 - 1.0
    },
    SessionCompleted {
        stats: SessionStats,
    },
    ErrorCorrected {
        position: usize,
    },
}

/// 会话统计数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    pub lesson_id: u32,
    pub wpm: f64,
    pub cpm: f64,
    pub accuracy: f64,
    pub total_keystrokes: usize,
    pub error_count: usize,
    #[serde(with = "duration_serde")]
    pub duration: Duration,
    pub timestamp: i64,
    pub weak_keys: Vec<(char, f32)>,
}

/// 中文拼音输入状态
#[derive(Debug, Clone, Default)]
pub struct PinyinState {
    pub buffer: String,
    pub candidates: Vec<char>,
    pub composing: bool,
}

// Duration 序列化辅助模块
mod duration_serde {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let tuple = (duration.as_secs(), duration.subsec_nanos());
        tuple.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let (secs, nanos) = <(u64, u32)>::deserialize(deserializer)?;
        Ok(Duration::new(secs, nanos))
    }
}
