use gpui::prelude::*;
use gpui::*;
use keyzen_core::*;
use keyzen_data::LessonLoader;
use keyzen_engine::TypingSession;
use keyzen_persistence::Database;
use std::sync::{mpsc, Arc};

// 定义 Actions
actions!(keyzen, [Quit, BackToList, ShowHistory]);

struct KeyzenApp {
    session: Option<Entity<SessionModel>>,
    lessons: Vec<Lesson>,
    selected_lesson: Option<usize>,
    focus_handle: FocusHandle,
    database: Arc<Database>,
    show_history: bool,
}

struct SessionModel {
    session: TypingSession,
    _event_rx: mpsc::Receiver<TypingEvent>,
}

impl SessionModel {
    fn new(lesson: Lesson, _cx: &mut Context<Self>) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let session = TypingSession::new(lesson, PracticeMode::Zen, Some(event_tx));

        Self {
            session,
            _event_rx: event_rx,
        }
    }

    fn handle_keystroke(&mut self, key: &str, cx: &mut Context<Self>) {
        // 处理退格键
        if key == "backspace" {
            self.session.handle_keystroke('\u{0008}');
            cx.notify();
            return;
        }

        // 处理普通字符
        if let Some(ch) = key.chars().next() {
            self.session.handle_keystroke(ch);
            cx.notify();
        }
    }

    fn get_target_text(&self) -> &str {
        self.session.get_target_text()
    }

    fn get_input_text(&self) -> String {
        self.session.get_input_text()
    }

    fn get_snapshot(&self) -> keyzen_engine::SessionSnapshot {
        self.session.get_snapshot()
    }

    fn is_completed(&self) -> bool {
        let snapshot = self.session.get_snapshot();
        snapshot.progress >= 1.0
    }
}

impl KeyzenApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let loader = LessonLoader::new("./lessons");
        let lessons = loader.load_all().unwrap_or_default();

        // 初始化数据库
        let database = Arc::new(Database::default().unwrap_or_else(|e| {
            eprintln!("警告: 无法创建数据库: {}", e);
            Database::new(":memory:").expect("无法创建内存数据库")
        }));

        Self {
            session: None,
            lessons,
            selected_lesson: None,
            focus_handle: cx.focus_handle(),
            database,
            show_history: false,
        }
    }

    fn start_lesson(&mut self, lesson_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(lesson) = self.lessons.get(lesson_index).cloned() {
            self.session = Some(cx.new(|cx| SessionModel::new(lesson, cx)));
            self.selected_lesson = Some(lesson_index);
            self.focus_handle.focus(window);
            cx.notify();
        }
    }

    fn back_to_list(&mut self, _: &BackToList, window: &mut Window, cx: &mut Context<Self>) {
        // 在清除 session 前保存数据
        if let Some(session) = &self.session {
            let db = self.database.clone();
            session.update(cx, |session_model, _cx| {
                if let Err(e) = session_model.session.save_to_database(&db) {
                    eprintln!("保存会话数据失败: {}", e);
                }
            });
        }

        self.session = None;
        self.selected_lesson = None;
        self.show_history = false;
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn show_history(&mut self, _: &ShowHistory, window: &mut Window, cx: &mut Context<Self>) {
        self.show_history = !self.show_history;
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn restart_lesson(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(lesson_index) = self.selected_lesson {
            if let Some(lesson) = self.lessons.get(lesson_index).cloned() {
                self.session = Some(cx.new(|cx| SessionModel::new(lesson, cx)));
                self.focus_handle.focus(window);
                cx.notify();
            }
        }
    }

    fn render_lesson_list(&self, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .h_full()
            .px_8()
            .child(
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xF0F0F0))
                            .child("选择课程"),
                    )
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .bg(rgb(0x2A2A2A))
                            .hover(|style| style.bg(rgb(0x3A3A3A)))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    this.show_history(&ShowHistory, window, cx);
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(0x00C2B8))
                                    .child("查看历史记录"),
                            ),
                    ),
            )
            .child(
                // 课程列表容器 - 可滚动
                uniform_list(
                    "lesson_list",
                    self.lessons.len(),
                    cx.processor(|this: &mut KeyzenApp, range, _window, cx| {
                        let mut items = Vec::new();
                        for i in range {
                            if let Some(lesson) = this.lessons.get(i).cloned() {
                                let lesson_index = i;

                                items.push(
                                    div()
                                        .id(i)
                                        .p_4()
                                        .bg(rgb(0x2A2A2A))
                                        .hover(|style| style.bg(rgb(0x3A3A3A)))
                                        .rounded(px(12.0))
                                        .cursor_pointer()
                                        .on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(move |this, _event, window, cx| {
                                                this.start_lesson(lesson_index, window, cx);
                                            }),
                                        )
                                        .child(
                                            div()
                                                .flex()
                                                .flex_col()
                                                .gap_2()
                                                .child(
                                                    div()
                                                        .text_size(px(16.0))
                                                        .font_weight(FontWeight::MEDIUM)
                                                        .text_color(rgb(0xF0F0F0))
                                                        .child(format!(
                                                            "{}. {}",
                                                            i + 1,
                                                            lesson.title
                                                        )),
                                                )
                                                .child(
                                                    div()
                                                        .text_size(px(14.0))
                                                        .text_color(rgb(0xA0A0A0))
                                                        .child(lesson.description),
                                                ),
                                        ),
                                );
                            }
                        }
                        items
                    }),
                )
                .flex_1(),
            )
    }

    fn render_history_view(&self, cx: &mut Context<Self>) -> Div {
        // 获取最近 10 条练习记录
        let sessions = self.database.get_recent_sessions(10).unwrap_or_default();
        let overall_stats = self.database.get_overall_stats().unwrap_or_else(|_| {
            keyzen_persistence::OverallStats {
                total_sessions: 0,
                total_keystrokes: 0,
                avg_wpm: 0.0,
                max_wpm: 0.0,
                avg_accuracy: 0.0,
            }
        });
        // 获取薄弱按键数据
        let weak_keys = self.database.get_overall_weak_keys(10).unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .h_full()
            .px_8()
            .child(
                // 标题栏
                div()
                    .flex()
                    .justify_between()
                    .items_center()
                    .child(
                        div()
                            .text_size(px(20.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xF0F0F0))
                            .child("练习历史"),
                    )
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .bg(rgb(0x2A2A2A))
                            .hover(|style| style.bg(rgb(0x3A3A3A)))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    this.show_history(&ShowHistory, window, cx);
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(0x00C2B8))
                                    .child("返回课程列表"),
                            ),
                    ),
            )
            .child(
                // 总体统计卡片
                div()
                    .w_full()
                    .p_6()
                    .bg(rgb(0x2A2A2A))
                    .rounded(px(12.0))
                    .child(
                        div()
                            .flex()
                            .justify_around()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(rgb(0xA0A0A0))
                                            .child("总练习次数"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xF0F0F0))
                                            .child(format!("{}", overall_stats.total_sessions)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(rgb(0xA0A0A0))
                                            .child("平均速度"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xF0F0F0))
                                            .child(format!("{:.0} WPM", overall_stats.avg_wpm)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(rgb(0xA0A0A0))
                                            .child("最高速度"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0x00C2B8))
                                            .child(format!("{:.0} WPM", overall_stats.max_wpm)),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_size(px(14.0))
                                            .text_color(rgb(0xA0A0A0))
                                            .child("平均准确率"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xF0F0F0))
                                            .child(format!(
                                                "{:.1}%",
                                                overall_stats.avg_accuracy * 100.0
                                            )),
                                    ),
                            ),
                    ),
            )
            .child(
                // 薄弱按键分析卡片
                div()
                    .w_full()
                    .p_6()
                    .bg(rgb(0x2A2A2A))
                    .rounded(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xF0F0F0))
                                    .child("薄弱按键分析"),
                            )
                            .child(if weak_keys.is_empty() {
                                div()
                                    .text_size(px(14.0))
                                    .text_color(rgb(0x666666))
                                    .child("暂无数据")
                            } else {
                                div().flex().flex_wrap().gap_3().children(
                                    weak_keys.into_iter().map(|weak_key| {
                                        // 根据错误率设置颜色
                                        let color = if weak_key.error_rate > 0.5 {
                                            rgb(0xFF6B6B) // 红色 - 高错误率
                                        } else if weak_key.error_rate > 0.3 {
                                            rgb(0xFFB86C) // 橙色 - 中错误率
                                        } else {
                                            rgb(0xFFD93D) // 黄色 - 低错误率
                                        };

                                        div()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .gap_1()
                                            .px_4()
                                            .py_3()
                                            .bg(rgb(0x1A1A1A))
                                            .rounded(px(8.0))
                                            .child(
                                                div()
                                                    .text_size(px(24.0))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(color)
                                                    .child(if weak_key.key_char == ' ' {
                                                        "␣".to_string()
                                                    } else if weak_key.key_char == '\n' {
                                                        "↵".to_string()
                                                    } else if weak_key.key_char == '\t' {
                                                        "⇥".to_string()
                                                    } else {
                                                        weak_key.key_char.to_string()
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(rgb(0xA0A0A0))
                                                    .child(format!(
                                                        "{:.0}%",
                                                        weak_key.error_rate * 100.0
                                                    )),
                                            )
                                    }),
                                )
                            }),
                    ),
            )
            .child(
                // 最近练习记录标题
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xF0F0F0))
                    .child("最近练习"),
            )
            .children(if sessions.is_empty() {
                vec![div()
                    .p_8()
                    .flex()
                    .justify_center()
                    .text_color(rgb(0x666666))
                    .child("暂无练习记录")]
            } else {
                sessions
                    .into_iter()
                    .map(|record| {
                        // 格式化时间
                        let datetime = chrono::DateTime::from_timestamp(record.completed_at, 0)
                            .unwrap_or_else(|| chrono::Utc::now());
                        let time_str = datetime.format("%Y-%m-%d %H:%M").to_string();

                        div()
                            .p_4()
                            .bg(rgb(0x2A2A2A))
                            .hover(|style| style.bg(rgb(0x3A3A3A)))
                            .rounded(px(12.0))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .items_center()
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .gap_2()
                                            .child(
                                                div()
                                                    .text_size(px(16.0))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(rgb(0xF0F0F0))
                                                    .child(record.lesson_title),
                                            )
                                            .child(
                                                div()
                                                    .text_size(px(12.0))
                                                    .text_color(rgb(0x666666))
                                                    .child(time_str),
                                            ),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .gap_6()
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .items_end()
                                                    .child(
                                                        div()
                                                            .text_size(px(14.0))
                                                            .text_color(rgb(0xA0A0A0))
                                                            .child("速度"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(18.0))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(rgb(0x00C2B8))
                                                            .child(format!("{:.0}", record.wpm)),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .flex_col()
                                                    .items_end()
                                                    .child(
                                                        div()
                                                            .text_size(px(14.0))
                                                            .text_color(rgb(0xA0A0A0))
                                                            .child("准确率"),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(18.0))
                                                            .font_weight(FontWeight::BOLD)
                                                            .text_color(rgb(0xF0F0F0))
                                                            .child(format!(
                                                                "{:.1}%",
                                                                record.accuracy * 100.0
                                                            )),
                                                    ),
                                            ),
                                    ),
                            )
                    })
                    .collect()
            })
    }

    fn render_practice_area(&self, session: &SessionModel) -> Div {
        let snapshot = session.get_snapshot();
        let target_text = session.get_target_text();
        let input_text = session.get_input_text();
        let target_chars: Vec<char> = target_text.chars().collect();
        let input_chars: Vec<char> = input_text.chars().collect();

        // 获取当前课程名称
        let lesson_title = self
            .selected_lesson
            .and_then(|idx| self.lessons.get(idx))
            .map(|lesson| lesson.title.clone())
            .unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .gap_8()
            .w_full()
            .px_8()
            .child(
                // 课程名称
                div()
                    .flex()
                    .justify_center()
                    .text_size(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xF0F0F0))
                    .child(lesson_title),
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .gap_8()
                    .text_sm()
                    .text_color(rgb(0xA0A0A0))
                    .child(format!("WPM: {:.0}", snapshot.current_wpm))
                    .child("|")
                    .child(format!("准确率: {:.1}%", snapshot.accuracy * 100.0))
                    .child("|")
                    .child(format!("进度: {:.0}%", snapshot.progress * 100.0)),
            )
            .child(
                div()
                    .w_full()
                    .p_12()
                    .bg(rgb(0x2A2A2A))
                    .rounded(px(16.0))
                    .child(
                        div()
                            .w_full()
                            .font_family("JetBrains Mono")
                            .text_size(px(24.0))
                            .line_height(px(36.0))
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .children(target_chars.iter().enumerate().map(|(i, &target_char)| {
                                let (color, bg_color) = if i < input_chars.len() {
                                    let input_char = input_chars[i];
                                    if input_char == target_char {
                                        (rgb(0xF0F0F0), None)
                                    } else {
                                        (rgb(0xFF9966), Some(rgb(0x2A2520)))
                                    }
                                } else if i == input_chars.len() {
                                    (rgb(0x000000), Some(rgb(0x00C2B8)))
                                } else {
                                    (rgb(0xA0A0A0), None)
                                };

                                let mut char_div = div()
                                    .h(px(36.0))
                                    .flex()
                                    .items_center()
                                    .text_color(color)
                                    .child(target_char.to_string());

                                if let Some(bg) = bg_color {
                                    char_div = char_div.bg(bg);
                                }

                                char_div
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .text_xs()
                    .text_color(rgb(0x666666))
                    .child("按 Esc 返回课程列表"),
            )
    }

    fn render_completion_stats(
        &self,
        snapshot: keyzen_engine::SessionSnapshot,
        cx: &mut Context<Self>,
    ) -> Div {
        // 获取当前课程名称
        let lesson_title = self
            .selected_lesson
            .and_then(|idx| self.lessons.get(idx))
            .map(|lesson| lesson.title.clone())
            .unwrap_or_default();

        div()
            .flex()
            .flex_col()
            .gap_8()
            .w_full()
            .px_8()
            .child(
                // 完成标题
                div()
                    .flex()
                    .justify_center()
                    .text_size(px(28.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0x00C2B8))
                    .child("课程完成！"),
            )
            .child(
                // 课程名称
                div()
                    .flex()
                    .justify_center()
                    .text_size(px(18.0))
                    .text_color(rgb(0xF0F0F0))
                    .child(lesson_title),
            )
            .child(
                // 统计数据卡片
                div()
                    .w_full()
                    .p_8()
                    .bg(rgb(0x2A2A2A))
                    .rounded(px(12.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_6()
                            .child(
                                // WPM
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .text_color(rgb(0xA0A0A0))
                                            .child("速度 (WPM)"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xF0F0F0))
                                            .child(format!("{:.0}", snapshot.current_wpm)),
                                    ),
                            )
                            .child(
                                // 准确率
                                div()
                                    .w_full()
                                    .flex()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_size(px(16.0))
                                            .text_color(rgb(0xA0A0A0))
                                            .child("准确率"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(0xF0F0F0))
                                            .child(format!("{:.1}%", snapshot.accuracy * 100.0)),
                                    ),
                            ),
                    ),
            )
            .child(
                // 操作按钮
                div()
                    .flex()
                    .gap_4()
                    .justify_center()
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .bg(rgb(0x00C2B8))
                            .hover(|style| style.bg(rgb(0x00A89F)))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    // 在重新开始前保存数据
                                    if let Some(session) = &this.session {
                                        let db = this.database.clone();
                                        session.update(cx, |session_model, _cx| {
                                            if let Err(e) =
                                                session_model.session.save_to_database(&db)
                                            {
                                                eprintln!("保存会话数据失败: {}", e);
                                            }
                                        });
                                    }
                                    this.restart_lesson(window, cx);
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0x000000))
                                    .child("重新练习"),
                            ),
                    )
                    .child(
                        div()
                            .px_6()
                            .py_3()
                            .bg(rgb(0x2A2A2A))
                            .hover(|style| style.bg(rgb(0x3A3A3A)))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    // 在清除 session 前保存数据
                                    if let Some(session) = &this.session {
                                        let db = this.database.clone();
                                        session.update(cx, |session_model, _cx| {
                                            if let Err(e) =
                                                session_model.session.save_to_database(&db)
                                            {
                                                eprintln!("保存会话数据失败: {}", e);
                                            }
                                        });
                                    }

                                    this.session = None;
                                    this.selected_lesson = None;
                                    this.focus_handle.focus(window);
                                    cx.notify();
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(0xF0F0F0))
                                    .child("返回课程列表"),
                            ),
                    ),
            )
    }
}

impl Render for KeyzenApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // 订阅 session 的变化
        if let Some(session) = &self.session {
            let session_clone = session.clone();
            cx.observe(&session_clone, |_, _, cx| {
                cx.notify();
            })
            .detach();
        }

        let content = if let Some(session) = &self.session {
            let is_completed = session.read(cx).is_completed();
            if is_completed {
                let snapshot = session.read(cx).get_snapshot();
                self.render_completion_stats(snapshot, cx)
            } else {
                let session_ref = session.read(cx);
                self.render_practice_area(session_ref)
            }
        } else if self.show_history {
            self.render_history_view(cx)
        } else {
            self.render_lesson_list(cx)
        };

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .bg(rgb(0x1A1A1A))
            .track_focus(&self.focus_handle)
            .key_context("KeyzenApp")
            .on_action(cx.listener(Self::back_to_list))
            .on_action(cx.listener(Self::show_history))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _window, cx| {
                if let Some(session) = &this.session {
                    let key = event.keystroke.key.as_str();

                    // 处理特殊功能键
                    match key {
                        "backspace" => {
                            session.update(cx, |session, cx| {
                                session.handle_keystroke("backspace", cx);
                            });
                            return;
                        }
                        "enter" => {
                            session.update(cx, |session, cx| {
                                session.handle_keystroke("\n", cx);
                            });
                            return;
                        }
                        "tab" => {
                            session.update(cx, |session, cx| {
                                session.handle_keystroke("\t", cx);
                            });
                            return;
                        }
                        "space" => {
                            session.update(cx, |session, cx| {
                                session.handle_keystroke(" ", cx);
                            });
                            return;
                        }
                        _ => {}
                    }

                    // 处理普通可打印字符（使用 key_char 以支持大小写和特殊符号）
                    if let Some(key_char) = &event.keystroke.key_char {
                        session.update(cx, |session, cx| {
                            session.handle_keystroke(key_char, cx);
                        });
                    }
                }
            }))
            .child(content)
    }
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

fn main() {
    Application::new().run(|cx: &mut App| {
        // 绑定快捷键
        cx.bind_keys([
            KeyBinding::new("escape", BackToList, Some("KeyzenApp")),
            KeyBinding::new("cmd-h", ShowHistory, Some("KeyzenApp")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);

        // 注册退出动作
        cx.on_action(quit);

        // 监听窗口关闭事件：当窗口关闭时退出应用
        cx.on_window_closed(|cx| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        // 创建应用菜单
        cx.set_menus(vec![Menu {
            name: "Keyzen".into(),
            items: vec![MenuItem::action("退出", Quit)],
        }]);

        // 打开窗口
        let bounds = Bounds::centered(None, size(px(900.0), px(650.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(600.0), px(500.0))),
                    app_id: Some("keyzen.pingfury.top".to_string()),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Keyzen - 键禅".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(|cx| KeyzenApp::new(cx)),
            )
            .unwrap();

        // 设置焦点并激活应用
        window
            .update(cx, |view, window, cx| {
                view.focus_handle.focus(window);
                cx.activate(true);
            })
            .unwrap();
    });
}
