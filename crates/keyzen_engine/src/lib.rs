use keyzen_core::*;
use log::debug;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[cfg(feature = "persistence")]
use keyzen_persistence::Database;

pub struct TypingSession {
    // 课程数据
    lesson: Lesson,
    _mode: PracticeMode,  // 保留供将来使用
    input_mode: InputMode,
    language: String, // 课程语言，用于统计计算

    // 新增：练习进度管理
    current_exercise_index: usize,      // 当前练习索引 (0-based)
    exercise_stats: Vec<ExerciseStats>, // 已完成练习的统计

    // 当前练习的输入状态
    target_chars: Vec<char>,
    input_chars: Vec<char>,
    current_position: usize,
    error_positions: HashSet<usize>,

    // 当前练习的统计数据
    exercise_start_time: Option<Instant>,
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
        // 从第一个练习初始化
        assert!(
            !lesson.exercises.is_empty(),
            "Lesson must have at least one exercise"
        );
        let first_exercise = &lesson.exercises[0];
        let target_chars: Vec<char> = first_exercise.content.chars().collect();
        let language = lesson.language.clone();

        Self {
            lesson,
            _mode: mode,
            input_mode: InputMode::default(),
            language,
            current_exercise_index: 0,
            exercise_stats: Vec::new(),
            target_chars,
            input_chars: Vec::new(),
            current_position: 0,
            error_positions: HashSet::new(),
            exercise_start_time: None,
            total_keystrokes: 0,
            correct_keystrokes: 0,
            keystroke_history: VecDeque::new(),
            event_tx,
        }
    }

    /// 获取当前练习
    pub fn get_current_exercise(&self) -> &Exercise {
        &self.lesson.exercises[self.current_exercise_index]
    }

    /// 获取进度 (当前索引, 总数)
    pub fn get_progress(&self) -> (usize, usize) {
        (self.current_exercise_index, self.lesson.exercises.len())
    }

    /// 是否还有下一个练习
    pub fn has_next_exercise(&self) -> bool {
        self.current_exercise_index + 1 < self.lesson.exercises.len()
    }

    /// 是否有上一个练习
    pub fn has_previous_exercise(&self) -> bool {
        self.current_exercise_index > 0
    }

    /// 跳转到上一个练习
    pub fn go_to_previous_exercise(&mut self) -> bool {
        if self.has_previous_exercise() {
            self.current_exercise_index -= 1;
            self.reset_for_current_exercise();
            true
        } else {
            false
        }
    }

    /// 手动跳转到下一个练习（不保存统计）
    pub fn go_to_next_exercise(&mut self) -> bool {
        if self.has_next_exercise() {
            self.current_exercise_index += 1;
            self.reset_for_current_exercise();
            true
        } else {
            false
        }
    }

    /// 当前练习是否完成
    pub fn is_current_exercise_complete(&self) -> bool {
        self.current_position >= self.target_chars.len()
    }

    /// 检查当前练习是否有错误
    pub fn current_exercise_has_errors(&self) -> bool {
        !self.error_positions.is_empty()
    }

    /// 完成当前练习，进入下一个
    pub fn advance_to_next_exercise(&mut self) -> bool {
        // 1. 生成当前练习的统计
        let stats = self.finalize_current_exercise();
        self.exercise_stats.push(stats);

        // 2. 检查是否还有下一个
        if self.has_next_exercise() {
            // 进入下一个练习
            self.current_exercise_index += 1;
            self.reset_for_next_exercise();
            true
        } else {
            // 所有练习完成
            false
        }
    }

    /// 重置状态以开始下一个练习
    fn reset_for_next_exercise(&mut self) {
        let exercise = self.get_current_exercise();
        self.target_chars = exercise.content.chars().collect();
        self.input_chars.clear();
        self.current_position = 0;
        self.error_positions.clear();
        self.exercise_start_time = None;
        self.total_keystrokes = 0;
        self.correct_keystrokes = 0;
        self.keystroke_history.clear();
    }

    /// 重置当前练习（用于手动跳转练习时）
    pub fn reset_for_current_exercise(&mut self) {
        let exercise = self.get_current_exercise();
        self.target_chars = exercise.content.chars().collect();
        self.input_chars.clear();
        self.current_position = 0;
        self.error_positions.clear();
        self.exercise_start_time = None;
        self.total_keystrokes = 0;
        self.correct_keystrokes = 0;
        self.keystroke_history.clear();
    }

    /// 根据记忆模式生成显示文本
    pub fn generate_display_text(&self, mode: MemoryMode) -> String {
        match mode {
            MemoryMode::Off => self.get_target_text().to_string(),
            MemoryMode::Complete => self.hide_complete(),
            MemoryMode::FirstLetter => self.hide_first_letter_only(),
            MemoryMode::Partial(level) => self.hide_partial(level),
        }
    }

    /// 完全隐藏：保留空格和标点，其他用 _ 替代
    fn hide_complete(&self) -> String {
        self.get_target_text()
            .chars()
            .map(|ch| {
                if ch.is_whitespace()
                    || ch.is_ascii_punctuation()
                    || "，。！？；：\"\"''（）【】《》、".contains(ch)
                {
                    ch
                } else {
                    '_'
                }
            })
            .collect()
    }

    /// 首字母提示模式
    fn hide_first_letter_only(&self) -> String {
        let text = self.get_target_text();

        if self.is_cjk_language() {
            // 中文：每个词显示第一个字
            self.hide_chinese_first_char(text)
        } else {
            // 英文：每个单词只显示首字母
            self.hide_english_first_letter(text)
        }
    }

    /// 部分隐藏模式
    fn hide_partial(&self, level: PartialLevel) -> String {
        let ratio = match level {
            PartialLevel::Low => 0.3,
            PartialLevel::Medium => 0.5,
            PartialLevel::High => 0.7,
        };

        if self.is_cjk_language() {
            // 中文：按字隐藏
            self.hide_chinese_chars(ratio)
        } else {
            // 英文/代码：按单词隐藏
            self.hide_english_words(ratio)
        }
    }

    /// 隐藏中文字符
    fn hide_chinese_chars(&self, ratio: f32) -> String {
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let text = self.get_target_text();
        let chars: Vec<char> = text.chars().collect();

        // 找出所有中文字符的索引
        let cjk_indices: Vec<usize> = chars
            .iter()
            .enumerate()
            .filter(|(_, &ch)| self.is_cjk_char(ch))
            .map(|(i, _)| i)
            .collect();

        // 计算需要隐藏的数量
        let hide_count = (cjk_indices.len() as f32 * ratio).round() as usize;

        // 随机选择要隐藏的索引
        let mut rng = thread_rng();
        let mut hide_indices: Vec<usize> = cjk_indices;
        hide_indices.shuffle(&mut rng);
        let hide_set: HashSet<usize> = hide_indices.into_iter().take(hide_count).collect();

        // 生成隐藏后的文本
        chars
            .iter()
            .enumerate()
            .map(|(i, &ch)| if hide_set.contains(&i) { '_' } else { ch })
            .collect()
    }

    /// 隐藏英文单词
    fn hide_english_words(&self, ratio: f32) -> String {
        use rand::seq::SliceRandom;
        use rand::thread_rng;

        let text = self.get_target_text();
        let mut words = Vec::new();
        let mut in_word = false;
        let mut start_idx = 0;

        // 提取所有单词的起始和结束位置
        for (i, ch) in text.chars().enumerate() {
            if ch.is_alphanumeric() {
                if !in_word {
                    in_word = true;
                    start_idx = i;
                }
            } else {
                if in_word {
                    in_word = false;
                    words.push((start_idx, i));
                }
            }
        }
        if in_word {
            words.push((start_idx, text.len()));
        }

        // 随机选择要隐藏的单词
        let hide_count = (words.len() as f32 * ratio).round() as usize;
        let mut rng = thread_rng();
        words.shuffle(&mut rng);
        let hide_words: HashSet<(usize, usize)> = words.into_iter().take(hide_count).collect();

        // 生成隐藏后的文本
        let chars: Vec<char> = text.chars().collect();
        let mut result = String::new();

        for (i, ch) in chars.iter().enumerate() {
            let should_hide = hide_words.iter().any(|&(start, end)| i >= start && i < end);
            if should_hide && ch.is_alphanumeric() {
                result.push('_');
            } else {
                result.push(*ch);
            }
        }
        result
    }

    /// 英文首字母提示
    fn hide_english_first_letter(&self, text: &str) -> String {
        let mut result = String::new();
        let mut in_word = false;
        let mut is_first = false;

        for ch in text.chars() {
            if ch.is_alphanumeric() {
                if !in_word {
                    in_word = true;
                    is_first = true;
                }
                if is_first {
                    result.push(ch);
                    is_first = false;
                } else {
                    result.push('_');
                }
            } else {
                in_word = false;
                result.push(ch);
            }
        }
        result
    }

    /// 中文首字提示：每个词显示第一个字
    fn hide_chinese_first_char(&self, text: &str) -> String {
        let mut result = String::new();
        let mut show_next = true;

        for ch in text.chars() {
            if self.is_cjk_char(ch) {
                if show_next {
                    result.push(ch);
                    show_next = false;
                } else {
                    result.push('_');
                }
            } else {
                result.push(ch);
                show_next = !ch.is_alphanumeric(); // 遇到标点/空格后重置
            }
        }
        result
    }

    /// 判断是否为 CJK 字符
    fn is_cjk_char(&self, ch: char) -> bool {
        matches!(ch,
            '\u{4E00}'..='\u{9FFF}' |  // CJK 统一表意文字
            '\u{3400}'..='\u{4DBF}' |  // CJK 扩展 A
            '\u{20000}'..='\u{2A6DF}' | // CJK 扩展 B
            '\u{2A700}'..='\u{2B73F}' | // CJK 扩展 C
            '\u{2B740}'..='\u{2B81F}' | // CJK 扩展 D
            '\u{2B820}'..='\u{2CEAF}' | // CJK 扩展 E
            '\u{F900}'..='\u{FAFF}' |   // CJK 兼容表意文字
            '\u{2F800}'..='\u{2FA1F}'   // CJK 兼容表意文字补充
        )
    }

    /// 核心方法：处理按键
    pub fn handle_keystroke(&mut self, ch: char) {
        debug!(
            "🟢 Engine::handle_keystroke 收到字符: {:?} (U+{:04X})",
            ch, ch as u32
        );

        // 首次按键启动计时
        if self.exercise_start_time.is_none() {
            self.exercise_start_time = Some(Instant::now());
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
            debug!("  ↳ 处理退格键");
            self.handle_backspace();
            return;
        }

        // 检查是否正确
        let target_char = self.target_chars.get(self.current_position);
        let is_correct = target_char == Some(&ch);

        debug!(
            "  ↳ 位置 {}: 目标={:?}, 输入={:?}, 正确={}",
            self.current_position, target_char, ch, is_correct
        );

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

        let chars = recent.len() as f64;
        let cpm = (chars / duration) * 60.0;

        // 根据语言调整 WPM 计算
        if self.is_cjk_language() {
            // CJK 语言: 1 个字符 = 1 个"词"
            cpm
        } else {
            // 拉丁字母语言: 平均 5 个字符 = 1 个词
            cpm / 5.0
        }
    }

    /// 判断是否为 CJK（中日韩）语言
    fn is_cjk_language(&self) -> bool {
        self.language.starts_with("zh-") // 中文
            || self.language.starts_with("ja-") // 日文
            || self.language.starts_with("ko-") // 韩文
    }

    /// 完成当前练习并生成统计
    fn finalize_current_exercise(&self) -> ExerciseStats {
        let duration = self
            .exercise_start_time
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

        // 根据语言调整 WPM 计算
        let wpm = if self.is_cjk_language() {
            cpm
        } else {
            cpm / 5.0
        };

        let exercise = self.get_current_exercise();
        ExerciseStats::from_exercise(
            exercise,
            self.current_exercise_index,
            wpm,
            accuracy,
            self.total_keystrokes,
            self.error_positions.len(),
            duration,
        )
    }

    /// 完成会话并生成统计（汇总所有练习）
    fn finalize_session(&self) -> SessionStats {
        // 构建所有练习的统计（包括已完成和当前的）
        let mut all_exercise_stats = self.exercise_stats.clone();

        // 如果当前练习已完成但还没添加到 exercise_stats，添加它
        if self.is_current_exercise_complete() {
            let current_stats = self.finalize_current_exercise();
            all_exercise_stats.push(current_stats);
        }

        // 汇总所有练习的数据
        let total_duration_secs: u64 = all_exercise_stats.iter().map(|s| s.duration_secs).sum();
        let total_keystrokes: usize = all_exercise_stats.iter().map(|s| s.total_keystrokes).sum();
        let total_errors: usize = all_exercise_stats.iter().map(|s| s.error_count).sum();

        let overall_accuracy = if total_keystrokes > 0 {
            (total_keystrokes - total_errors) as f64 / total_keystrokes as f64
        } else {
            0.0
        };

        let overall_cpm = if total_duration_secs > 0 {
            ((total_keystrokes - total_errors) as f64 / total_duration_secs as f64) * 60.0
        } else {
            0.0
        };

        let overall_wpm = if self.is_cjk_language() {
            overall_cpm
        } else {
            overall_cpm / 5.0
        };

        // 提取薄弱单元（基于所有练习）
        let weak_units = self.extract_weak_units();

        SessionStats {
            lesson_id: self.lesson.id,
            exercise_stats: all_exercise_stats,
            overall_wpm,
            overall_cpm,
            overall_accuracy,
            total_keystrokes,
            error_count: total_errors,
            duration_secs: total_duration_secs,
            timestamp: chrono::Utc::now().timestamp(),
            weak_units,
        }
    }

    /// 根据课程语言类型提取薄弱单元
    fn extract_weak_units(&self) -> Vec<WeakUnit> {
        match self.language.as_str() {
            lang if lang.starts_with("zh-") => self.extract_chinese_weak_units(),
            lang if lang.starts_with("en-") => self.extract_english_weak_units(),
            "rust" | "python" | "javascript" => self.extract_code_weak_units(),
            _ => self.extract_character_weak_units(), // 默认字符级别
        }
    }

    /// 中文：提取单字符（汉字）和常见双字词组
    fn extract_chinese_weak_units(&self) -> Vec<WeakUnit> {
        let mut unit_stats: HashMap<String, (usize, usize, UnitType)> = HashMap::new();

        // 1. 单字符统计
        for (i, &target_char) in self.target_chars.iter().enumerate() {
            let key = target_char.to_string();
            let entry = unit_stats.entry(key).or_insert((0, 0, UnitType::Character));
            entry.0 += 1; // 总次数
            if self.error_positions.contains(&i) {
                entry.1 += 1; // 错误次数
            }
        }

        // 2. 双字词组统计（可选）
        for i in 0..self.target_chars.len().saturating_sub(1) {
            let c1 = self.target_chars[i];
            let c2 = self.target_chars[i + 1];

            // 只统计双汉字组合
            if c1.is_ascii() || c2.is_ascii() || c1.is_whitespace() || c2.is_whitespace() {
                continue;
            }

            let phrase = format!("{}{}", c1, c2);
            let has_error =
                self.error_positions.contains(&i) || self.error_positions.contains(&(i + 1));

            let entry = unit_stats.entry(phrase).or_insert((0, 0, UnitType::Phrase));
            entry.0 += 1;
            if has_error {
                entry.1 += 1;
            }
        }

        self.build_weak_units_from_stats(unit_stats)
    }

    /// 英文：提取单词级别
    fn extract_english_weak_units(&self) -> Vec<WeakUnit> {
        let mut unit_stats: HashMap<String, (usize, usize, UnitType)> = HashMap::new();

        // 分词逻辑
        let target_text = self.target_chars.iter().collect::<String>();
        let words: Vec<&str> = target_text.split_whitespace().collect();

        let mut char_offset = 0;
        for word in words {
            let word_start = char_offset;
            let word_end = char_offset + word.len();

            // 检查该单词是否有错误
            let has_error = (word_start..word_end).any(|i| self.error_positions.contains(&i));

            let entry = unit_stats
                .entry(word.to_string())
                .or_insert((0, 0, UnitType::Word));
            entry.0 += 1;
            if has_error {
                entry.1 += 1;
            }

            // 跳过单词和后面的空格
            char_offset = word_end;
            // 查找下一个非空白字符的位置
            while char_offset < self.target_chars.len()
                && self.target_chars[char_offset].is_whitespace()
            {
                char_offset += 1;
            }
        }

        // 同时也统计字符级别（用于特殊字符和标点）
        for (i, &target_char) in self.target_chars.iter().enumerate() {
            // 只统计非字母数字的字符
            if !target_char.is_alphanumeric() && !target_char.is_whitespace() {
                let key = target_char.to_string();
                let entry = unit_stats.entry(key).or_insert((0, 0, UnitType::Character));
                entry.0 += 1;
                if self.error_positions.contains(&i) {
                    entry.1 += 1;
                }
            }
        }

        self.build_weak_units_from_stats(unit_stats)
    }

    /// 代码：提取字符级别（可扩展为 token 级别）
    fn extract_code_weak_units(&self) -> Vec<WeakUnit> {
        // 暂时使用字符级别，后续可扩展为 token 级别
        self.extract_character_weak_units()
    }

    /// 默认：字符级别统计
    fn extract_character_weak_units(&self) -> Vec<WeakUnit> {
        let mut unit_stats: HashMap<String, (usize, usize, UnitType)> = HashMap::new();

        for (i, &target_char) in self.target_chars.iter().enumerate() {
            let key = target_char.to_string();
            let entry = unit_stats.entry(key).or_insert((0, 0, UnitType::Character));
            entry.0 += 1;
            if self.error_positions.contains(&i) {
                entry.1 += 1;
            }
        }

        self.build_weak_units_from_stats(unit_stats)
    }

    /// 从统计数据构建 WeakUnit 列表
    fn build_weak_units_from_stats(
        &self,
        stats: HashMap<String, (usize, usize, UnitType)>,
    ) -> Vec<WeakUnit> {
        let mut units: Vec<WeakUnit> = stats
            .into_iter()
            .filter(|(_, (total, _, _))| *total >= 3) // 至少出现 3 次
            .map(|(content, (total, errors, unit_type))| {
                let error_rate = errors as f32 / total as f32;
                WeakUnit {
                    content,
                    unit_type,
                    error_count: errors,
                    total_count: total,
                    error_rate,
                }
            })
            .filter(|unit| unit.error_rate > 0.15) // 错误率 > 15%
            .collect();

        units.sort_by(|a, b| b.error_rate.partial_cmp(&a.error_rate).unwrap());
        units.truncate(10); // 保留前 10 个
        units
    }

    /// 获取 UI 渲染用的快照
    pub fn get_snapshot(&self) -> SessionSnapshot {
        // 计算整个 session 的进度（所有练习）
        let total_exercises = self.lesson.exercises.len();
        let completed_exercises = self.exercise_stats.len();
        let current_exercise_progress = if !self.target_chars.is_empty() {
            self.current_position as f32 / self.target_chars.len() as f32
        } else {
            0.0
        };

        let overall_progress = if total_exercises > 0 {
            (completed_exercises as f32 + current_exercise_progress) / total_exercises as f32
        } else {
            0.0
        };

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
            progress: overall_progress,
        }
    }

    /// 获取当前练习的目标文本
    pub fn get_target_text(&self) -> &str {
        &self.get_current_exercise().content
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
