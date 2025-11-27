use keyzen_core::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(feature = "persistence")]
use keyzen_persistence::Database;

pub struct TypingSession {
    // 课程数据
    lesson: Lesson,
    mode: PracticeMode,
    input_mode: InputMode,

    // 输入状态
    target_chars: Vec<char>,
    input_chars: Vec<char>,
    current_position: usize,
    error_positions: HashSet<usize>,

    // 统计数据
    start_time: Option<Instant>,
    total_keystrokes: usize,
    correct_keystrokes: usize,
    keystroke_history: VecDeque<(Instant, char, bool)>,

    // 事件发布
    event_tx: Option<mpsc::Sender<TypingEvent>>,
}

impl TypingSession {
    pub fn new(
        lesson: Lesson,
        mode: PracticeMode,
        event_tx: Option<mpsc::Sender<TypingEvent>>,
    ) -> Self {
        let target_chars: Vec<char> = lesson.source_text.chars().collect();

        Self {
            lesson,
            mode,
            input_mode: InputMode::default(),
            target_chars,
            input_chars: Vec::new(),
            current_position: 0,
            error_positions: HashSet::new(),
            start_time: None,
            total_keystrokes: 0,
            correct_keystrokes: 0,
            keystroke_history: VecDeque::new(),
            event_tx,
        }
    }

    /// 核心方法：处理按键
    pub fn handle_keystroke(&mut self, ch: char) {
        // 首次按键启动计时
        if self.start_time.is_none() {
            self.start_time = Some(Instant::now());
        }

        let now = Instant::now();
        self.total_keystrokes += 1;

        // 所有语言使用系统输入法，直接处理字符
        self.handle_char_input(ch, now);
    }

    /// 处理字符输入
    fn handle_char_input(&mut self, ch: char, now: Instant) {
        // 处理退格键
        if ch == '\u{0008}' {
            self.handle_backspace();
            return;
        }

        // 检查是否正确
        let target_char = self.target_chars.get(self.current_position);
        let is_correct = target_char == Some(&ch);

        if is_correct {
            self.correct_keystrokes += 1;
            self.input_chars.push(ch);
            self.error_positions.remove(&self.current_position);
            self.current_position += 1;

            // 发送事件
            self.send_event(TypingEvent::KeyPressed {
                char: ch,
                correct: true,
                position: self.current_position - 1,
            });

            // 检查是否完成单词
            if ch == ' ' || ch == '\n' {
                let wpm = self.calculate_current_wpm();
                self.send_event(TypingEvent::WordCompleted { wpm });
            }

            // 检查是否完成课程
            if self.current_position >= self.target_chars.len() {
                let stats = self.finalize_session();
                self.send_event(TypingEvent::SessionCompleted { stats });
            }
        } else {
            // 错误处理
            match self.input_mode {
                InputMode::Strict => {
                    // 严格模式：不允许继续
                    self.error_positions.insert(self.current_position);
                }
                InputMode::Forgiving => {
                    // 宽容模式：标记但继续
                    self.error_positions.insert(self.current_position);
                    self.input_chars.push(ch);
                    self.current_position += 1;
                }
                InputMode::Invisible => {
                    // 隐形模式：不显示错误
                    self.input_chars.push(ch);
                    self.current_position += 1;
                }
            }

            self.send_event(TypingEvent::KeyPressed {
                char: ch,
                correct: false,
                position: if self.current_position > 0 {
                    self.current_position - 1
                } else {
                    0
                },
            });
        }

        // 记录历史（用于 WPM 计算）
        self.keystroke_history.push_back((now, ch, is_correct));

        // 只保留最近 10 秒的历史
        while let Some((ts, _, _)) = self.keystroke_history.front() {
            if now.duration_since(*ts) > Duration::from_secs(10) {
                self.keystroke_history.pop_front();
            } else {
                break;
            }
        }
    }

    fn handle_backspace(&mut self) {
        if self.current_position > 0 {
            self.current_position -= 1;
            self.input_chars.pop();

            if self.error_positions.remove(&self.current_position) {
                self.send_event(TypingEvent::ErrorCorrected {
                    position: self.current_position,
                });
            }
        }
    }

    /// 计算当前 WPM（基于最近 10 秒）
    fn calculate_current_wpm(&self) -> f64 {
        if self.keystroke_history.is_empty() {
            return 0.0;
        }

        let now = Instant::now();
        let recent: Vec<_> = self
            .keystroke_history
            .iter()
            .filter(|(ts, _, correct)| *correct && now.duration_since(*ts).as_secs() <= 10)
            .collect();

        if recent.is_empty() {
            return 0.0;
        }

        let first_ts = recent.first().unwrap().0;
        let duration = now.duration_since(first_ts).as_secs_f64();

        if duration < 0.1 {
            return 0.0;
        }

        // WPM = (字符数 / 5) / (秒数 / 60)
        let chars = recent.len() as f64;
        (chars / 5.0) / (duration / 60.0)
    }

    /// 完成会话并生成统计
    fn finalize_session(&self) -> SessionStats {
        let duration = self
            .start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO);

        let accuracy = if self.total_keystrokes > 0 {
            self.correct_keystrokes as f64 / self.total_keystrokes as f64
        } else {
            0.0
        };

        let cpm = if duration.as_secs() > 0 {
            (self.correct_keystrokes as f64 / duration.as_secs_f64()) * 60.0
        } else {
            0.0
        };

        let wpm = cpm / 5.0;

        // 统计薄弱按键
        let weak_keys = self.calculate_weak_keys();

        SessionStats {
            lesson_id: self.lesson.id,
            wpm,
            cpm,
            accuracy,
            total_keystrokes: self.total_keystrokes,
            error_count: self.error_positions.len(),
            duration,
            timestamp: chrono::Utc::now().timestamp(),
            weak_keys,
        }
    }

    fn calculate_weak_keys(&self) -> Vec<(char, f32)> {
        let mut key_stats: HashMap<char, (usize, usize)> = HashMap::new();

        for (i, &ch) in self.target_chars.iter().enumerate() {
            let entry = key_stats.entry(ch).or_insert((0, 0));
            entry.0 += 1; // 总次数
            if self.error_positions.contains(&i) {
                entry.1 += 1; // 错误次数
            }
        }

        let mut weak: Vec<_> = key_stats
            .into_iter()
            .filter(|(_, (total, _))| *total >= 3) // 至少出现 3 次
            .map(|(ch, (total, errors))| (ch, errors as f32 / total as f32))
            .filter(|(_, rate)| *rate > 0.2) // 错误率 > 20%
            .collect();

        weak.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        weak.truncate(5); // 只保留前 5 个
        weak
    }

    /// 获取 UI 渲染用的快照
    pub fn get_snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            cursor_position: self.current_position,
            recent_errors: self
                .error_positions
                .iter()
                .filter(|&&pos| pos >= self.current_position.saturating_sub(50))
                .copied()
                .collect(),
            current_wpm: self.calculate_current_wpm(),
            accuracy: if self.total_keystrokes > 0 {
                self.correct_keystrokes as f64 / self.total_keystrokes as f64
            } else {
                1.0
            },
            progress: if !self.target_chars.is_empty() {
                self.current_position as f32 / self.target_chars.len() as f32
            } else {
                0.0
            },
        }
    }

    /// 获取目标文本
    pub fn get_target_text(&self) -> &str {
        &self.lesson.source_text
    }

    /// 获取已输入的文本
    pub fn get_input_text(&self) -> String {
        self.input_chars.iter().collect()
    }

    fn send_event(&self, event: TypingEvent) {
        if let Some(tx) = &self.event_tx {
            let _ = tx.send(event);
        }
    }

    /// 保存会话到数据库（需要启用 persistence feature）
    #[cfg(feature = "persistence")]
    pub fn save_to_database(&self, db: &Database) -> Result<i64, Box<dyn std::error::Error>> {
        let stats = self.finalize_session();
        let session_id = db.save_session(&stats, &self.lesson.title)?;
        Ok(session_id)
    }

    /// 获取课程标题
    pub fn get_lesson_title(&self) -> &str {
        &self.lesson.title
    }
}

/// UI 渲染快照（轻量级）
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub cursor_position: usize,
    pub recent_errors: Vec<usize>,
    pub current_wpm: f64,
    pub accuracy: f64,
    pub progress: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use keyzen_core::{Difficulty, LessonMeta};

    fn create_test_lesson() -> Lesson {
        Lesson {
            id: 1,
            lesson_type: LessonType::Prose,
            language: "en-US".to_string(),
            title: "Test Lesson".to_string(),
            description: "A test lesson".to_string(),
            source_text: "hello world".to_string(),
            meta: LessonMeta {
                difficulty: Difficulty::Beginner,
                tags: vec!["test".to_string()],
                estimated_time: Duration::from_secs(60),
                prerequisite_ids: vec![],
            },
        }
    }

    #[test]
    fn test_typing_session_creation() {
        let lesson = create_test_lesson();
        let session = TypingSession::new(lesson, PracticeMode::Zen, None);
        assert_eq!(session.current_position, 0);
        assert_eq!(session.total_keystrokes, 0);
    }

    #[test]
    fn test_correct_keystroke() {
        let lesson = create_test_lesson();
        let mut session = TypingSession::new(lesson, PracticeMode::Zen, None);

        session.handle_keystroke('h');
        assert_eq!(session.current_position, 1);
        assert_eq!(session.correct_keystrokes, 1);
        assert_eq!(session.error_positions.len(), 0);
    }

    #[test]
    fn test_incorrect_keystroke_forgiving() {
        let lesson = create_test_lesson();
        let mut session = TypingSession::new(lesson, PracticeMode::Zen, None);

        session.handle_keystroke('x'); // 错误输入
        assert_eq!(session.current_position, 1); // Forgiving 模式继续
        assert_eq!(session.error_positions.len(), 1);
    }
}
