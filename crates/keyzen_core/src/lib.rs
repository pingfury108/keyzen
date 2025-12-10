use serde::{Deserialize, Serialize};
use std::time::Duration;

/// 课程类型
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum LessonType {
    Prose,        // 散文
    Code,         // 代码
    SpecialChars, // 特殊符号
    Chinese,      // 中文练习（使用系统输入法）
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
    Strict,    // 必须纠正错误
    Forgiving, // 标记错误但可继续（默认）
    Invisible, // 不显示错误（盲打）
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

/// 统计单元类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnitType {
    Character, // 单字符（包括单个汉字）
    Word,      // 单词（英文）
    Phrase,    // 短语/词组（中文）
    Token,     // 代码 token（函数名、关键字等）
}

impl UnitType {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnitType::Character => "character",
            UnitType::Word => "word",
            UnitType::Phrase => "phrase",
            UnitType::Token => "token",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "word" => UnitType::Word,
            "phrase" => UnitType::Phrase,
            "token" => UnitType::Token,
            _ => UnitType::Character,
        }
    }
}

/// 薄弱单元
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeakUnit {
    pub content: String,     // 内容（可以是单字符或多字符）
    pub unit_type: UnitType, // 单元类型
    pub error_count: usize,  // 错误次数
    pub total_count: usize,  // 总出现次数
    pub error_rate: f32,     // 错误率
}

impl WeakUnit {
    pub fn new(content: String, unit_type: UnitType) -> Self {
        Self {
            content,
            unit_type,
            error_count: 0,
            total_count: 0,
            error_rate: 0.0,
        }
    }

    pub fn calculate_error_rate(&mut self) {
        if self.total_count == 0 {
            self.error_rate = 0.0;
        } else {
            self.error_rate = self.error_count as f32 / self.total_count as f32;
        }
    }
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
    pub weak_units: Vec<WeakUnit>, // 修改为 WeakUnit
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
