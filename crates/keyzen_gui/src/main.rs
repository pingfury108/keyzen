use gpui::prelude::*;
use gpui::*;
use keyzen_core::*;
use keyzen_data::LessonLoader;
use keyzen_engine::TypingSession;
use keyzen_persistence::{Database, SessionRecord};
use log::debug;
use std::ops::Range;
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
    // 缓存历史记录,用于列表渲染
    cached_sessions: Vec<SessionRecord>,
    // 用于 InputHandler
    practice_area_bounds: Option<Bounds<Pixels>>,
}

struct SessionModel {
    session: TypingSession,
    _event_rx: mpsc::Receiver<TypingEvent>,
}

// 自定义 Element 用于注册 InputHandler
struct PracticeAreaElement {
    app: Entity<KeyzenApp>,
    content: Div,
}

impl IntoElement for PracticeAreaElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for PracticeAreaElement {
    type RequestLayoutState = <Div as Element>::RequestLayoutState;
    type PrepaintState = <Div as Element>::PrepaintState;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        self.content.request_layout(id, inspector_id, window, cx)
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.content
            .prepaint(id, inspector_id, bounds, request_layout, window, cx)
    }

    fn paint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        // 注册 InputHandler
        let focus_handle = self.app.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.app.clone()),
            cx,
        );

        // 保存边界供 InputHandler 使用
        self.app.update(cx, |app, _cx| {
            app.practice_area_bounds = Some(bounds);
        });

        // 绘制内容
        self.content.paint(
            id,
            inspector_id,
            bounds,
            request_layout,
            prepaint,
            window,
            cx,
        )
    }
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
            cached_sessions: Vec::new(),
            practice_area_bounds: None,
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
        if self.show_history {
            // 加载历史记录
            self.cached_sessions = self.database.get_recent_sessions(10).unwrap_or_default();
        }
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

    fn render_lesson_list(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_col()
            .gap_6()
            .w_full()
            .h_full()
            .p_8()
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
                                    div().id(i).px_8().py_2().child(
                                        div()
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
                                    ),
                                );
                            }
                        }
                        items
                    }),
                )
                .flex_1(),
            )
            .into_any()
    }

    fn render_history_view(&self, cx: &mut Context<Self>) -> AnyElement {
        // 获取总体统计
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
            .p_8()
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
            .when(self.cached_sessions.is_empty(), |el| {
                el.child(
                    div()
                        .p_8()
                        .flex()
                        .justify_center()
                        .text_color(rgb(0x666666))
                        .child("暂无练习记录")
                )
            })
            .when(!self.cached_sessions.is_empty(), |el| {
                el.child(
                    uniform_list(
                        "history_list",
                        self.cached_sessions.len(),
                        cx.processor(|this: &mut KeyzenApp, range, _window, _cx| {
                            let mut items = Vec::new();
                            for i in range {
                                if let Some(record) = this.cached_sessions.get(i).cloned() {
                                    let record: SessionRecord = record; // Re-introducing this line
                                    let lesson_title = record.lesson_title;
                                    let completed_at = {
                                        let datetime = chrono::DateTime::from_timestamp(record.completed_at, 0)
                                            .unwrap_or_else(|| chrono::Utc::now());
                                        datetime.format("%Y-%m-%d %H:%M").to_string()
                                    };
                                    let wpm = format!("{:.0}", record.wpm);
                                    let accuracy = format!("{:.1}%", record.accuracy * 100.0);

                                    items.push(
                                        div()
                                            .id(i)
                                            .py_2()
                                            .child(
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
                                                                            .child(lesson_title),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(12.0))
                                                                            .text_color(rgb(0x666666))
                                                                            .child(completed_at),
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
                                                                                    .child(wpm),
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
                                                                                    .child(accuracy),
                                                                            ),
                                                                    ),
                                                            ),
                                                    ),
                                            )
                                    );
                                }
                            }
                            items
                        }),
                    )
                    .flex_1()
                )
            })
            .into_any()
    }

    fn render_practice_area(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let (snapshot, target_text, input_text) = if let Some(session) = &self.session {
            let session_read = session.read(cx);
            (
                session_read.get_snapshot(),
                session_read.get_target_text().to_string(),
                session_read.get_input_text(),
            )
        } else {
            return div().into_any();
        };

        let target_chars: Vec<char> = target_text.chars().collect();
        let input_chars: Vec<char> = input_text.chars().collect();

        // 获取当前课程名称
        let lesson_title = self
            .selected_lesson
            .and_then(|idx| self.lessons.get(idx))
            .map(|lesson| lesson.title.clone())
            .unwrap_or_default();

        let content = div()
            .flex()
            .flex_col()
            .gap_8()
            .w_full()
            .p_8()
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
            );

        PracticeAreaElement {
            app: cx.entity(),
            content,
        }
        .into_any()
    }

    fn render_completion_stats(
        &self,
        snapshot: keyzen_engine::SessionSnapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            .p_8()
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
            .into_any()
    }
}

// 实现 EntityInputHandler 来处理 IME 输入
impl EntityInputHandler for KeyzenApp {
    fn text_for_range(
        &mut self,
        _range: Range<usize>,
        _adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<String> {
        // 我们不需要支持文本范围查询，因为我们是只写入的
        None
    }

    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        // 我们不需要选区功能
        None
    }

    fn marked_text_range(
        &self,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        // 我们不需要标记文本（IME 正在输入的文本）
        None
    }

    fn unmark_text(&mut self, _window: &mut Window, _cx: &mut Context<Self>) {
        // 不需要实现
    }

    fn replace_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        text: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // 这是关键方法：当 IME 提交最终文本时会调用这里
        // text 参数包含 IME 确认后的最终文本（比如汉字"你好"）
        debug!(
            "🔵 InputHandler::replace_text_in_range 收到文本: {:?}",
            text
        );

        if let Some(session) = &self.session {
            // 遍历文本中的每个字符并处理
            for ch in text.chars() {
                debug!("  ↳ 处理字符: {:?} (U+{:04X})", ch, ch as u32);
                session.update(cx, |session, cx| {
                    session.handle_keystroke(&ch.to_string(), cx);
                });
            }
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _range: Option<Range<usize>>,
        _new_text: &str,
        _new_selected_range: Option<Range<usize>>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        // 这个方法在 IME 输入过程中（还未确认）会被调用
        // 我们不处理中间状态，只等待最终确认
    }

    fn bounds_for_range(
        &mut self,
        _range: Range<usize>,
        _element_bounds: Bounds<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        // 返回练习区域的边界，用于 IME 候选窗口定位
        self.practice_area_bounds
    }

    fn character_index_for_point(
        &mut self,
        _point: Point<Pixels>,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) -> Option<usize> {
        None
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
                self.render_practice_area(cx)
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
                // 只处理功能键，不处理可打印字符
                // 可打印字符（包括 IME 输入的汉字）由 InputHandler::replace_text_in_range 处理
                let key = event.keystroke.key.as_str();
                let key_char = event.keystroke.key_char.as_deref();
                debug!("🟡 on_key_down: key={:?}, key_char={:?}", key, key_char);

                if let Some(session) = &this.session {
                    // 只处理特殊功能键
                    // 注意：Space 键不在这里处理！
                    // Space 在 IME 输入时用于选择候选词，最终字符由 InputHandler 提交
                    match key {
                        "backspace" => {
                            debug!("  ↳ 处理功能键: Backspace");
                            session.update(cx, |session, cx| {
                                session.handle_keystroke("backspace", cx);
                            });
                        }
                        "enter" => {
                            debug!("  ↳ 处理功能键: Enter");
                            session.update(cx, |session, cx| {
                                session.handle_keystroke("\n", cx);
                            });
                        }
                        "tab" => {
                            debug!("  ↳ 处理功能键: Tab");
                            session.update(cx, |session, cx| {
                                session.handle_keystroke("\t", cx);
                            });
                        }
                        _ => {
                            debug!("  ↳ 忽略按键，等待 InputHandler");
                        }
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
    // 初始化日志系统
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug"))
        .format_timestamp_millis()
        .init();

    debug!("🚀 Keyzen GUI 启动");

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
