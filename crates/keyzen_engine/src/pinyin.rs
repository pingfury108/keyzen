use keyzen_core::PinyinState;

/// 拼音输入处理器
pub struct PinyinProcessor {
    state: PinyinState,
}

impl PinyinProcessor {
    pub fn new() -> Self {
        Self {
            state: PinyinState::default(),
        }
    }

    /// 获取当前状态
    pub fn state(&self) -> &PinyinState {
        &self.state
    }

    /// 处理输入字符
    pub fn handle_input(&mut self, ch: char) -> PinyinInputResult {
        match ch {
            // 字母输入：添加到拼音缓冲区
            'a'..='z' | 'A'..='Z' => {
                self.state.buffer.push(ch.to_ascii_lowercase());
                self.state.composing = true;
                self.update_candidates();
                PinyinInputResult::Composing
            }
            // 空格：选择第一个候选字
            ' ' => self.select_candidate(0),
            // 数字 1-9：选择对应候选字
            '1'..='9' => {
                let index = (ch as u8 - b'1') as usize;
                self.select_candidate(index)
            }
            // 退格：删除拼音缓冲区最后一个字符
            '\u{0008}' => {
                if self.state.buffer.is_empty() {
                    PinyinInputResult::PassThrough(ch)
                } else {
                    self.state.buffer.pop();
                    if self.state.buffer.is_empty() {
                        self.clear();
                    } else {
                        self.update_candidates();
                    }
                    PinyinInputResult::Composing
                }
            }
            // 其他字符：直接透传
            _ => {
                if self.state.composing {
                    self.clear();
                }
                PinyinInputResult::PassThrough(ch)
            }
        }
    }

    /// 更新候选字列表
    fn update_candidates(&mut self) {
        if self.state.buffer.is_empty() {
            self.state.candidates.clear();
            return;
        }

        // 使用 pinyin crate 获取候选字
        // 注意：这里需要一个拼音到汉字的字典
        // 暂时使用简单的实现，后续可以加载完整字典
        self.state.candidates = self.get_candidates_for_pinyin(&self.state.buffer);
    }

    /// 根据拼音获取候选字
    fn get_candidates_for_pinyin(&self, pinyin_str: &str) -> Vec<char> {
        // 常用拼音到汉字的映射（示例数据）
        // 实际应该从完整字典加载
        let common_chars: Vec<(Vec<&str>, Vec<char>)> = vec![
            (vec!["ni"], vec!['你', '尼', '泥']),
            (vec!["hao"], vec!['好', '号', '浩']),
            (vec!["ma"], vec!['吗', '马', '妈']),
            (vec!["wo"], vec!['我', '握', '沃']),
            (vec!["de"], vec!['的', '得', '地']),
            (vec!["shi"], vec!['是', '时', '事']),
            (vec!["yi"], vec!['一', '已', '以']),
            (vec!["ge"], vec!['个', '各', '歌']),
            (vec!["zhe"], vec!['这', '者', '着']),
            (vec!["le"], vec!['了', '乐', '勒']),
            (vec!["ta"], vec!['他', '她', '它']),
            (vec!["men"], vec!['们', '门', '闷']),
            (vec!["ren"], vec!['人', '任', '认']),
            (vec!["zai"], vec!['在', '再', '载']),
            (vec!["you"], vec!['有', '又', '友']),
            (vec!["lai"], vec!['来', '莱', '赖']),
            (vec!["qu"], vec!['去', '取', '趣']),
            (vec!["kan"], vec!['看', '刊', '砍']),
            (vec!["shuo"], vec!['说', '硕', '烁']),
            (vec!["dao"], vec!['到', '道', '倒']),
        ];

        // 查找匹配的拼音
        for (pinyins, chars) in &common_chars {
            if pinyins.contains(&pinyin_str) {
                return chars.clone();
            }
        }

        Vec::new()
    }

    /// 选择候选字
    fn select_candidate(&mut self, index: usize) -> PinyinInputResult {
        if !self.state.composing || self.state.candidates.is_empty() {
            return PinyinInputResult::PassThrough(' ');
        }

        if let Some(&ch) = self.state.candidates.get(index) {
            self.clear();
            PinyinInputResult::Commit(ch)
        } else {
            PinyinInputResult::Composing
        }
    }

    /// 清空状态
    fn clear(&mut self) {
        self.state.buffer.clear();
        self.state.candidates.clear();
        self.state.composing = false;
    }

    /// 重置处理器
    pub fn reset(&mut self) {
        self.clear();
    }
}

impl Default for PinyinProcessor {
    fn default() -> Self {
        Self::new()
    }
}

/// 拼音输入处理结果
#[derive(Debug, Clone, PartialEq)]
pub enum PinyinInputResult {
    /// 正在组合中，不输出字符
    Composing,
    /// 提交字符
    Commit(char),
    /// 透传字符（非拼音输入）
    PassThrough(char),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_pinyin_input() {
        let mut processor = PinyinProcessor::new();

        // 输入 "ni"
        assert_eq!(processor.handle_input('n'), PinyinInputResult::Composing);
        assert_eq!(processor.handle_input('i'), PinyinInputResult::Composing);
        assert!(processor.state().composing);
        assert_eq!(processor.state().buffer, "ni");
        assert!(!processor.state().candidates.is_empty());

        // 空格选择第一个候选字
        let result = processor.handle_input(' ');
        assert!(matches!(result, PinyinInputResult::Commit('你')));
        assert!(!processor.state().composing);
        assert!(processor.state().buffer.is_empty());
    }

    #[test]
    fn test_number_selection() {
        let mut processor = PinyinProcessor::new();

        // 输入 "hao"
        processor.handle_input('h');
        processor.handle_input('a');
        processor.handle_input('o');

        // 使用数字 2 选择第二个候选字
        let result = processor.handle_input('2');
        if let PinyinInputResult::Commit(ch) = result {
            assert_eq!(ch, '号');
        } else {
            panic!("Expected Commit result");
        }
    }

    #[test]
    fn test_backspace() {
        let mut processor = PinyinProcessor::new();

        // 输入 "ni"
        processor.handle_input('n');
        processor.handle_input('i');
        assert_eq!(processor.state().buffer, "ni");

        // 退格删除一个字符
        processor.handle_input('\u{0008}');
        assert_eq!(processor.state().buffer, "n");

        // 再次退格
        processor.handle_input('\u{0008}');
        assert_eq!(processor.state().buffer, "");
        assert!(!processor.state().composing);
    }

    #[test]
    fn test_passthrough() {
        let mut processor = PinyinProcessor::new();

        // 非字母字符应该透传
        let result = processor.handle_input('!');
        assert_eq!(result, PinyinInputResult::PassThrough('!'));
    }

    #[test]
    fn test_reset() {
        let mut processor = PinyinProcessor::new();

        processor.handle_input('n');
        processor.handle_input('i');
        assert!(processor.state().composing);

        processor.reset();
        assert!(!processor.state().composing);
        assert!(processor.state().buffer.is_empty());
    }
}
