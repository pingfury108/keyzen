use gpui::prelude::*;
use gpui::*;
use keyzen_core::*;
use keyzen_data::LessonLoader;
use keyzen_engine::TypingSession;
use keyzen_persistence::{Database, SessionRecord};
use log::debug;
use std::ops::Range;
use std::sync::{mpsc, Arc, Mutex};

// 定义 Actions
actions!(
    keyzen,
    [Quit, BackToList, ShowHistory, ShowSettings, ToggleTheme]
);

// 主题枚举
#[derive(Debug, Clone, Copy, PartialEq)]
enum Theme {
    Dark,
    Light,
}

// 主题颜色
struct ThemeColors {
    bg_primary: Hsla,
    bg_secondary: Hsla,
    bg_hover: Hsla,
    text_primary: Hsla,
    text_secondary: Hsla,
    text_muted: Hsla,
    accent: Hsla,
    error: Hsla,
    error_bg: Hsla,
    cursor: Hsla,
}

struct KeyzenApp {
    session: Option<Entity<SessionModel>>,
    lessons: Vec<Lesson>,
    lesson_loader: LessonLoader,
    needs_reload: Arc<Mutex<bool>>, // 标记是否需要重新加载
    selected_lesson: Option<usize>,
    focus_handle: FocusHandle,
    database: Arc<Database>,
    show_history: bool,
    show_settings: bool,
    current_theme: Theme,
    memory_mode: MemoryMode,
    // 缓存完成时的统计快照（避免 WPM 持续变化）
    completion_snapshot: Option<keyzen_engine::SessionSnapshot>,
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

            // 检查当前练习是否完成且无错误，才自动跳转
            if self.session.is_current_exercise_complete()
                && !self.session.current_exercise_has_errors()
            {
                if self.session.has_next_exercise() {
                    self.session.advance_to_next_exercise();
                    debug!("✅ 练习无错误，自动跳转到下一个练习");
                    cx.notify();
                }
            }
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

    fn generate_display_text(&self, mode: MemoryMode) -> String {
        self.session.generate_display_text(mode)
    }

    fn is_completed(&self) -> bool {
        let snapshot = self.session.get_snapshot();
        snapshot.progress >= 1.0
    }
}

impl KeyzenApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let loader = LessonLoader::new("./lessons").unwrap_or_else(|e| {
            eprintln!("❌ 初始化课程加载器失败: {}", e);
            panic!("无法初始化课程加载器");
        });

        let lessons = match loader.load_all() {
            Ok(lessons) => {
                debug!("✅ 成功加载 {} 个课程", lessons.len());
                for lesson in &lessons {
                    debug!(
                        "  - [{}] {}: {} 个练习",
                        lesson.id,
                        lesson.title,
                        lesson.exercises.len()
                    );
                }
                lessons
            }
            Err(e) => {
                eprintln!("❌ 加载课程失败: {}", e);
                debug!("❌ 加载课程失败: {:?}", e);
                Vec::new()
            }
        };

        // 初始化数据库
        let database = Arc::new(Database::default().unwrap_or_else(|e| {
            eprintln!("警告: 无法创建数据库: {}", e);
            Database::new(":memory:").expect("无法创建内存数据库")
        }));

        // 从数据库加载主题配置
        let current_theme = database
            .get_config("theme")
            .ok()
            .flatten()
            .and_then(|s| match s.as_str() {
                "light" => Some(Theme::Light),
                "dark" => Some(Theme::Dark),
                _ => None,
            })
            .unwrap_or(Theme::Dark); // 默认深色主题

        // 从数据库加载记忆模式配置
        let memory_mode = database
            .get_config("memory_mode")
            .ok()
            .flatten()
            .and_then(|s| {
                // 解析格式：off, complete, first_letter, partial_low, partial_medium, partial_high
                match s.as_str() {
                    "off" => Some(MemoryMode::Off),
                    "complete" => Some(MemoryMode::Complete),
                    "first_letter" => Some(MemoryMode::FirstLetter),
                    "partial_low" => Some(MemoryMode::Partial(PartialLevel::Low)),
                    "partial_medium" => Some(MemoryMode::Partial(PartialLevel::Medium)),
                    "partial_high" => Some(MemoryMode::Partial(PartialLevel::High)),
                    _ => None,
                }
            })
            .unwrap_or(MemoryMode::Off); // 默认关闭

        let needs_reload = Arc::new(Mutex::new(false));
        let needs_reload_clone = needs_reload.clone();

        let mut app = Self {
            session: None,
            lessons,
            lesson_loader: loader,
            needs_reload,
            selected_lesson: None,
            focus_handle: cx.focus_handle(),
            database,
            show_history: false,
            show_settings: false,
            current_theme,
            memory_mode,
            completion_snapshot: None,
            cached_sessions: Vec::new(),
            practice_area_bounds: None,
        };

        // 启动文件监听
        if let Err(e) = app.lesson_loader.start_watching(move || {
            *needs_reload_clone.lock().unwrap() = true;
            debug!("📂 检测到用户课程文件变化，标记需要重新加载");
        }) {
            eprintln!("⚠️  启动课程文件监听失败: {}", e);
        }

        app
    }

    /// 重新加载课程
    fn reload_lessons(&mut self, cx: &mut Context<Self>) {
        debug!("🔄 重新加载课程...");

        match self.lesson_loader.load_all() {
            Ok(new_lessons) => {
                self.lessons = new_lessons;
                debug!("✅ 课程已重新加载: {} 个", self.lessons.len());

                // 如果当前正在练习的课程索引超出范围，返回主页
                if let Some(idx) = self.selected_lesson {
                    if idx >= self.lessons.len() {
                        self.session = None;
                        self.selected_lesson = None;
                        debug!("⚠️  当前课程已失效，返回主页");
                    }
                }

                cx.notify();
            }
            Err(e) => {
                eprintln!("❌ 重新加载课程失败: {}", e);
            }
        }
    }

    fn start_lesson(&mut self, lesson_index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(lesson) = self.lessons.get(lesson_index).cloned() {
            self.session = Some(cx.new(|cx| SessionModel::new(lesson, cx)));
            self.selected_lesson = Some(lesson_index);
            self.completion_snapshot = None; // 清除之前的完成快照
            self.focus_handle.focus(window);
            cx.notify();
        }
    }

    fn back_to_list(&mut self, _: &BackToList, window: &mut Window, cx: &mut Context<Self>) {
        // 如果在设置页面，Esc 关闭设置
        if self.show_settings {
            self.show_settings = false;
            self.focus_handle.focus(window);
            cx.notify();
            return;
        }

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
        self.completion_snapshot = None; // 清除完成快照
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

    fn show_settings(&mut self, _: &ShowSettings, window: &mut Window, cx: &mut Context<Self>) {
        self.show_settings = !self.show_settings;
        self.focus_handle.focus(window);
        cx.notify();
    }

    fn toggle_theme(&mut self, _: &ToggleTheme, _window: &mut Window, cx: &mut Context<Self>) {
        self.current_theme = match self.current_theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        };

        // 保存主题配置到数据库
        let theme_str = match self.current_theme {
            Theme::Dark => "dark",
            Theme::Light => "light",
        };
        if let Err(e) = self.database.save_config("theme", theme_str) {
            eprintln!("保存主题配置失败: {}", e);
        }

        cx.notify();
    }

    fn set_memory_mode(&mut self, mode: MemoryMode, cx: &mut Context<Self>) {
        self.memory_mode = mode;

        // 保存记忆模式配置到数据库
        let mode_str = match mode {
            MemoryMode::Off => "off",
            MemoryMode::Complete => "complete",
            MemoryMode::FirstLetter => "first_letter",
            MemoryMode::Partial(PartialLevel::Low) => "partial_low",
            MemoryMode::Partial(PartialLevel::Medium) => "partial_medium",
            MemoryMode::Partial(PartialLevel::High) => "partial_high",
        };
        if let Err(e) = self.database.save_config("memory_mode", mode_str) {
            eprintln!("保存记忆模式配置失败: {}", e);
        }

        cx.notify();
    }

    // 获取主题颜色
    fn get_colors(&self) -> ThemeColors {
        match self.current_theme {
            Theme::Dark => ThemeColors {
                bg_primary: rgb(0x1A1A1A).into(),
                bg_secondary: rgb(0x2A2A2A).into(),
                bg_hover: rgb(0x3A3A3A).into(),
                text_primary: rgb(0xF0F0F0).into(),
                text_secondary: rgb(0xA0A0A0).into(),
                text_muted: rgb(0x666666).into(),
                accent: rgb(0x00C2B8).into(),
                error: rgb(0xFF9966).into(),
                error_bg: rgb(0x2A2520).into(),
                cursor: rgb(0x00C2B8).into(),
            },
            Theme::Light => ThemeColors {
                bg_primary: rgb(0xFAFAFA).into(),
                bg_secondary: rgb(0xF0F0F0).into(),
                bg_hover: rgb(0xE5E5E5).into(),
                text_primary: rgb(0x2A2A2A).into(),
                text_secondary: rgb(0x666666).into(),
                text_muted: rgb(0xA0A0A0).into(),
                accent: rgb(0x0080FF).into(),
                error: rgb(0xFF6B35).into(),
                error_bg: rgb(0xFFE5D9).into(),
                cursor: rgb(0x0080FF).into(),
            },
        }
    }

    fn restart_lesson(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(lesson_index) = self.selected_lesson {
            if let Some(lesson) = self.lessons.get(lesson_index).cloned() {
                self.session = Some(cx.new(|cx| SessionModel::new(lesson, cx)));
                self.completion_snapshot = None; // 清除完成快照
                self.focus_handle.focus(window);
                cx.notify();
            }
        }
    }

    fn render_lesson_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.get_colors();

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
                            .text_color(colors.text_primary)
                            .child("选择课程"),
                    )
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .bg(colors.bg_secondary)
                            .hover(|style| style.bg(colors.bg_hover))
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
                                    .text_color(colors.accent)
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
                        let colors = this.get_colors();
                        let mut items = Vec::new();
                        for i in range {
                            if let Some(lesson) = this.lessons.get(i).cloned() {
                                let lesson_index = i;

                                items.push(
                                    div().id(i).px_8().py_2().child(
                                        div()
                                            .p_4()
                                            .bg(colors.bg_secondary)
                                            .hover(|style| style.bg(colors.bg_hover))
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
                                                            .text_color(colors.text_primary)
                                                            .child(format!(
                                                                "{}. {}",
                                                                i + 1,
                                                                lesson.title
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .text_size(px(14.0))
                                                            .text_color(colors.text_secondary)
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
        let colors = self.get_colors();

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
        // 获取薄弱单元数据（词云）
        let weak_units = self.database.get_overall_weak_units(20).unwrap_or_default();

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
                            .text_color(colors.text_primary)
                            .child("练习历史"),
                    )
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .bg(colors.bg_secondary)
                            .hover(|style| style.bg(colors.bg_hover))
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
                                    .text_color(colors.accent)
                                    .child("返回课程列表"),
                            ),
                    ),
            )
            .child(
                // 总体统计卡片
                div()
                    .w_full()
                    .p_6()
                    .bg(colors.bg_secondary)
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
                                            .text_color(colors.text_secondary)
                                            .child("总练习次数"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(colors.text_primary)
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
                                            .text_color(colors.text_secondary)
                                            .child("平均速度"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(colors.text_primary)
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
                                            .text_color(colors.text_secondary)
                                            .child("最高速度"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(colors.accent)
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
                                            .text_color(colors.text_secondary)
                                            .child("平均准确率"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(colors.text_primary)
                                            .child(format!(
                                                "{:.1}%",
                                                overall_stats.avg_accuracy * 100.0
                                            )),
                                    ),
                            ),
                    ),
            )
            .when(!weak_units.is_empty(), |this| {
                this.child(
                    // 薄弱模式词云（仅在有数据时显示）
                    div()
                        .w_full()
                        .p_6()
                        .bg(colors.bg_secondary)
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
                                        .text_color(colors.text_primary)
                                        .child("薄弱模式识别"),
                                )
                                .child(self.render_word_cloud(weak_units, &colors))
                        ),
                )
            })
            .child(
                // 最近练习记录标题
                div()
                    .text_size(px(16.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(colors.text_primary)
                    .child("最近练习"),
            )
            .when(self.cached_sessions.is_empty(), |el| {
                el.child(
                    div()
                        .p_8()
                        .flex()
                        .justify_center()
                        .text_color(colors.text_muted)
                        .child("暂无练习记录")
                )
            })
            .when(!self.cached_sessions.is_empty(), |el| {
                el.child(
                    uniform_list(
                        "history_list",
                        self.cached_sessions.len(),
                        cx.processor(|this: &mut KeyzenApp, range, _window, _cx| {
                            let colors = this.get_colors();
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
                                                    .bg(colors.bg_secondary)
                                                    .hover(|style| style.bg(colors.bg_hover))
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
                                                                            .text_color(colors.text_primary)
                                                                            .child(lesson_title),
                                                                    )
                                                                    .child(
                                                                        div()
                                                                            .text_size(px(12.0))
                                                                            .text_color(colors.text_muted)
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
                                                                                    .text_color(colors.text_secondary)
                                                                                    .child("速度"),
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_size(px(18.0))
                                                                                    .font_weight(FontWeight::BOLD)
                                                                                    .text_color(colors.accent)
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
                                                                                    .text_color(colors.text_secondary)
                                                                                    .child("准确率"),
                                                                            )
                                                                            .child(
                                                                                div()
                                                                                    .text_size(px(18.0))
                                                                                    .font_weight(FontWeight::BOLD)
                                                                                    .text_color(colors.text_primary)
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
        let colors = self.get_colors();

        let (snapshot, target_text, display_text, input_text, progress, current_exercise) =
            if let Some(session) = &self.session {
                let session_read = session.read(cx);
                let (current, total) = session_read.session.get_progress();
                (
                    session_read.get_snapshot(),
                    session_read.get_target_text().to_string(),
                    session_read.generate_display_text(self.memory_mode),
                    session_read.get_input_text(),
                    (current, total),
                    session_read.session.get_current_exercise().clone(),
                )
            } else {
                return div().into_any();
            };

        let target_chars: Vec<char> = target_text.chars().collect();
        let display_chars: Vec<char> = display_text.chars().collect();
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
            .w_full()
            .h_full()
            .child(
                // 固定在顶部的信息区域
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .p_8()
                    .pb_4()
                    .child(
                        // 课程名称
                        div()
                            .flex()
                            .justify_center()
                            .text_size(px(18.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(colors.text_primary)
                            .child(lesson_title),
                    )
                    .child(
                        // 练习进度 + 导航按钮
                        div()
                            .flex()
                            .justify_center()
                            .items_center()
                            .gap_4()
                            .child(
                                // 上一个按钮
                                div()
                                    .px_3()
                                    .py_1()
                                    .bg(if progress.0 > 0 {
                                        colors.bg_secondary
                                    } else {
                                        colors.bg_primary
                                    })
                                    .when(progress.0 > 0, |el| {
                                        el.hover(|style| style.bg(colors.bg_hover))
                                    })
                                    .rounded(px(6.0))
                                    .cursor(if progress.0 > 0 {
                                        gpui::CursorStyle::PointingHand
                                    } else {
                                        gpui::CursorStyle::Arrow
                                    })
                                    .when(progress.0 > 0, |el| {
                                        el.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                if let Some(session) = &this.session {
                                                    session.update(cx, |session_model, cx| {
                                                        session_model
                                                            .session
                                                            .go_to_previous_exercise();
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(if progress.0 > 0 {
                                                colors.text_secondary
                                            } else {
                                                colors.text_muted
                                            })
                                            .child("← 上一个"),
                                    ),
                            )
                            .child(
                                // 进度文字
                                div()
                                    .text_size(px(14.0))
                                    .text_color(colors.text_secondary)
                                    .child(format!("练习 {}/{}", progress.0 + 1, progress.1)),
                            )
                            .child(
                                // 下一个按钮
                                div()
                                    .px_3()
                                    .py_1()
                                    .bg(if progress.0 + 1 < progress.1 {
                                        colors.bg_secondary
                                    } else {
                                        colors.bg_primary
                                    })
                                    .when(progress.0 + 1 < progress.1, |el| {
                                        el.hover(|style| style.bg(colors.bg_hover))
                                    })
                                    .rounded(px(6.0))
                                    .cursor(if progress.0 + 1 < progress.1 {
                                        gpui::CursorStyle::PointingHand
                                    } else {
                                        gpui::CursorStyle::Arrow
                                    })
                                    .when(progress.0 + 1 < progress.1, |el| {
                                        el.on_mouse_down(
                                            MouseButton::Left,
                                            cx.listener(|this, _event, _window, cx| {
                                                if let Some(session) = &this.session {
                                                    session.update(cx, |session_model, cx| {
                                                        session_model.session.go_to_next_exercise();
                                                        cx.notify();
                                                    });
                                                }
                                            }),
                                        )
                                    })
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(if progress.0 + 1 < progress.1 {
                                                colors.text_secondary
                                            } else {
                                                colors.text_muted
                                            })
                                            .child("下一个 →"),
                                    ),
                            ),
                    )
                    .child(
                        // 统计信息
                        div()
                            .flex()
                            .justify_center()
                            .gap_8()
                            .text_sm()
                            .font_family("JetBrains Mono")
                            .text_color(colors.text_secondary)
                            .child(
                                div().flex().gap_1().child("WPM:").child(
                                    div()
                                        .w(px(36.0))
                                        .text_align(TextAlign::Right)
                                        .child(format!("{:.0}", snapshot.current_wpm)),
                                ),
                            )
                            .child("|")
                            .child(
                                div().flex().gap_1().child("准确率:").child(
                                    div()
                                        .w(px(60.0))
                                        .text_align(TextAlign::Right)
                                        .child(format!("{:.1}%", snapshot.accuracy * 100.0)),
                                ),
                            )
                            .child("|")
                            .child(
                                div().flex().gap_1().child("进度:").child(
                                    div()
                                        .w(px(48.0))
                                        .text_align(TextAlign::Right)
                                        .child(format!("{:.0}%", snapshot.progress * 100.0)),
                                ),
                            ),
                    ),
            )
            .child(
                // 打字区域（占据剩余空间）
                div().flex_1().px_8().pb_4().child(
                    div()
                        .w_full()
                        .p_12()
                        .bg(colors.bg_secondary)
                        .rounded(px(16.0))
                        .flex()
                        .flex_col()
                        .gap_4()
                        .when(current_exercise.hint.is_some(), |el| {
                            el.child(
                                // 提示信息 - 左对齐
                                div()
                                    .text_size(px(13.0))
                                    .text_color(colors.text_muted)
                                    .child(current_exercise.hint.as_ref().unwrap().clone()),
                            )
                        })
                        .child(
                            // 打字文本
                            div()
                                .w_full()
                                .font_family("JetBrains Mono")
                                .text_size(px(24.0))
                                .line_height(px(36.0))
                                .flex()
                                .flex_row()
                                .flex_wrap()
                                .children(display_chars.iter().enumerate().map(
                                    |(i, &display_char)| {
                                        let target_char =
                                            target_chars.get(i).copied().unwrap_or(' ');

                                        // 决定显示什么字符：已正确输入的显示真实字符，其他显示隐藏字符
                                        let show_char = if i < input_chars.len() {
                                            let input_char = input_chars[i];
                                            if input_char == target_char {
                                                target_char // 输入正确，显示真实字符
                                            } else {
                                                display_char // 输入错误，显示隐藏字符（会标红）
                                            }
                                        } else {
                                            display_char // 未输入，显示隐藏字符
                                        };

                                        let (color, bg_color) = if i < input_chars.len() {
                                            let input_char = input_chars[i];
                                            if input_char == target_char {
                                                (colors.text_primary, None)
                                            } else {
                                                (colors.error, Some(colors.error_bg))
                                            }
                                        } else if i == input_chars.len() {
                                            (rgb(0x000000).into(), Some(colors.cursor))
                                        } else {
                                            (colors.text_secondary, None)
                                        };

                                        let mut char_div = div()
                                            .h(px(36.0))
                                            .flex()
                                            .items_center()
                                            .text_color(color)
                                            .child(show_char.to_string());

                                        if let Some(bg) = bg_color {
                                            char_div = char_div.bg(bg);
                                        }

                                        char_div
                                    },
                                )),
                        ),
                ),
            )
            .child(
                // 固定在底部的提示
                div()
                    .px_8()
                    .pb_8()
                    .pt_4()
                    .flex()
                    .justify_center()
                    .text_xs()
                    .text_color(colors.text_muted)
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
        let colors = self.get_colors();

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
                    .text_color(colors.accent)
                    .child("课程完成！"),
            )
            .child(
                // 课程名称
                div()
                    .flex()
                    .justify_center()
                    .text_size(px(18.0))
                    .text_color(colors.text_primary)
                    .child(lesson_title),
            )
            .child(
                // 统计数据卡片
                div()
                    .w_full()
                    .p_8()
                    .bg(colors.bg_secondary)
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
                                            .text_color(colors.text_secondary)
                                            .child("速度 (WPM)"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(colors.text_primary)
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
                                            .text_color(colors.text_secondary)
                                            .child("准确率"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(24.0))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(colors.text_primary)
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
                            .bg(colors.accent)
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
                            .bg(colors.bg_secondary)
                            .hover(|style| style.bg(colors.bg_hover))
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
                                    .text_color(colors.text_primary)
                                    .child("返回课程列表"),
                            ),
                    ),
            )
            .into_any()
    }

    /// 渲染词云组件
    fn render_word_cloud(
        &self,
        weak_units: Vec<WeakUnit>,
        colors: &ThemeColors,
    ) -> impl IntoElement {
        // 计算字体大小范围
        let max_error_rate = weak_units
            .iter()
            .map(|u| u.error_rate)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(1.0);

        let min_error_rate = weak_units
            .iter()
            .map(|u| u.error_rate)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        // 渲染词云
        div()
            .flex()
            .flex_wrap() // 关键：自动换行
            .gap_3()
            .justify_center()
            .items_center()
            .p_4()
            .children(weak_units.into_iter().map(|unit| {
                self.render_word_cloud_item(unit, min_error_rate, max_error_rate, colors)
            }))
    }

    /// 渲染单个词云项
    fn render_word_cloud_item(
        &self,
        unit: WeakUnit,
        min_error_rate: f32,
        max_error_rate: f32,
        colors: &ThemeColors,
    ) -> impl IntoElement {
        // 1. 计算字体大小（线性映射 14px ~ 32px）
        let normalized = if max_error_rate > min_error_rate {
            (unit.error_rate - min_error_rate) / (max_error_rate - min_error_rate)
        } else {
            1.0
        };
        let font_size = 14.0 + normalized * 18.0; // 14px ~ 32px

        // 2. 计算颜色（错误率越高颜色越深）
        let color = if unit.error_rate > 0.5 {
            rgb(0xFF4757) // 深红
        } else if unit.error_rate > 0.35 {
            rgb(0xFF6B6B) // 红色
        } else if unit.error_rate > 0.25 {
            rgb(0xFFB86C) // 橙色
        } else {
            rgb(0xFFD93D) // 黄色
        };

        // 3. 格式化显示内容
        let display_content = match unit.content.as_str() {
            " " => "␣".to_string(),
            "\n" => "↵".to_string(),
            "\t" => "⇥".to_string(),
            _ => unit.content.clone(),
        };

        // 5. 渲染
        div()
            .px_3()
            .py_2()
            .bg(colors.bg_primary.opacity(0.5))
            .rounded(px(8.0))
            .hover(|style| style.bg(colors.bg_hover))
            .cursor_pointer()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .child(
                        // 内容文字
                        div()
                            .text_size(px(font_size))
                            .font_weight(FontWeight::BOLD)
                            .text_color(color)
                            .child(display_content),
                    )
                    .child(
                        // 错误率标签（小字）
                        div()
                            .text_size(px(10.0))
                            .text_color(colors.text_muted)
                            .child(format!("{:.0}%", unit.error_rate * 100.0)),
                    ),
            )
    }

    /// 渲染记忆模式按钮
    fn render_memory_mode_button(
        &self,
        mode: MemoryMode,
        label: &str,
        colors: &ThemeColors,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let is_selected = self.memory_mode == mode;
        let label_owned = label.to_string();

        div()
            .px_4()
            .py_2()
            .bg(if is_selected {
                colors.accent
            } else {
                colors.bg_primary
            })
            .when(!is_selected, |el| {
                el.hover(|style| style.bg(colors.bg_hover))
            })
            .rounded(px(6.0))
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event, _window, cx| {
                    if this.memory_mode != mode {
                        this.set_memory_mode(mode, cx);
                    }
                }),
            )
            .child(
                div()
                    .text_size(px(13.0))
                    .text_color(if is_selected {
                        if matches!(self.current_theme, Theme::Light) {
                            rgb(0xFFFFFF) // 浅色主题选中时白色文字
                        } else {
                            rgb(0x000000) // 深色主题选中时黑色文字
                        }
                    } else {
                        colors.text_secondary.into()
                    })
                    .child(label_owned),
            )
    }

    fn render_settings_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let colors = self.get_colors();
        let is_dark = self.current_theme == Theme::Dark;

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
                            .text_color(colors.text_primary)
                            .child("设置"),
                    )
                    .child(
                        div()
                            .px_4()
                            .py_2()
                            .bg(colors.bg_secondary)
                            .hover(|style| style.bg(colors.bg_hover))
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(|this, _event, window, cx| {
                                    this.show_settings(&ShowSettings, window, cx);
                                }),
                            )
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .text_color(colors.accent)
                                    .child("关闭"),
                            ),
                    ),
            )
            .child(
                // 设置内容区域
                div()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .flex_1()
                    .child(
                        // 外观设置
                        div()
                            .w_full()
                            .p_6()
                            .bg(colors.bg_secondary)
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
                                            .text_color(colors.text_primary)
                                            .child("外观"),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .justify_between()
                                            .items_center()
                                            .child(
                                                div()
                                                    .text_size(px(14.0))
                                                    .text_color(colors.text_secondary)
                                                    .child("主题"),
                                            )
                                            .child(
                                                div()
                                                    .flex()
                                                    .gap_2()
                                                    .child(
                                                        // 深色主题按钮
                                                        div()
                                                            .px_4()
                                                            .py_2()
                                                            .bg(if is_dark {
                                                                colors.accent
                                                            } else {
                                                                colors.bg_primary
                                                            })
                                                            .when(!is_dark, |el| {
                                                                el.hover(|style| style.bg(colors.bg_hover))
                                                            })
                                                            .rounded(px(6.0))
                                                            .cursor_pointer()
                                                            .on_mouse_down(
                                                                MouseButton::Left,
                                                                cx.listener(|this, _event, _window, cx| {
                                                                    if this.current_theme != Theme::Dark {
                                                                        this.current_theme = Theme::Dark;
                                                                        // 保存主题配置
                                                                        if let Err(e) = this.database.save_config("theme", "dark") {
                                                                            eprintln!("保存主题配置失败: {}", e);
                                                                        }
                                                                        cx.notify();
                                                                    }
                                                                }),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(14.0))
                                                                    .text_color(if is_dark {
                                                                        rgb(0x000000) // 深色主题选中时黑色文字
                                                                    } else {
                                                                        colors.text_secondary.into()
                                                                    })
                                                                    .child("深色"),
                                                            ),
                                                    )
                                                    .child(
                                                        // 浅色主题按钮
                                                        div()
                                                            .px_4()
                                                            .py_2()
                                                            .bg(if !is_dark {
                                                                colors.accent
                                                            } else {
                                                                colors.bg_primary
                                                            })
                                                            .when(is_dark, |el| {
                                                                el.hover(|style| style.bg(colors.bg_hover))
                                                            })
                                                            .rounded(px(6.0))
                                                            .cursor_pointer()
                                                            .on_mouse_down(
                                                                MouseButton::Left,
                                                                cx.listener(|this, _event, _window, cx| {
                                                                    if this.current_theme != Theme::Light {
                                                                        this.current_theme = Theme::Light;
                                                                        // 保存主题配置
                                                                        if let Err(e) = this.database.save_config("theme", "light") {
                                                                            eprintln!("保存主题配置失败: {}", e);
                                                                        }
                                                                        cx.notify();
                                                                    }
                                                                }),
                                                            )
                                                            .child(
                                                                div()
                                                                    .text_size(px(14.0))
                                                                    .text_color(if !is_dark {
                                                                        rgb(0xFFFFFF) // 浅色主题选中时白色文字
                                                                    } else {
                                                                        colors.text_secondary.into()
                                                                    })
                                                                    .child("浅色"),
                                                            ),
                                                    ),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        // 记忆模式设置
                        div()
                            .w_full()
                            .p_6()
                            .bg(colors.bg_secondary)
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
                                            .text_color(colors.text_primary)
                                            .child("记忆模式"),
                                    )
                                    .child(
                                        div()
                                            .text_size(px(13.0))
                                            .text_color(colors.text_muted)
                                            .child("隐藏部分或全部文本以练习记忆打字"),
                                    )
                                    .child(
                                        // 记忆模式选项
                                        div()
                                            .flex()
                                            .flex_wrap()
                                            .gap_2()
                                            .child(self.render_memory_mode_button(MemoryMode::Off, "关闭", &colors, cx))
                                            .child(self.render_memory_mode_button(MemoryMode::FirstLetter, "首字母提示", &colors, cx))
                                            .child(self.render_memory_mode_button(MemoryMode::Partial(PartialLevel::Low), "部分隐藏 (30%)", &colors, cx))
                                            .child(self.render_memory_mode_button(MemoryMode::Partial(PartialLevel::Medium), "部分隐藏 (50%)", &colors, cx))
                                            .child(self.render_memory_mode_button(MemoryMode::Partial(PartialLevel::High), "部分隐藏 (70%)", &colors, cx))
                                            .child(self.render_memory_mode_button(MemoryMode::Complete, "完全隐藏", &colors, cx)),
                                    ),
                            ),
                    )
                    .child(
                        // 提示文本
                        div()
                            .flex()
                            .justify_center()
                            .text_xs()
                            .text_color(colors.text_muted)
                            .child("按 Esc 关闭设置"),
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
                session.update(cx, |session_model, cx| {
                    session_model.handle_keystroke(&ch.to_string(), cx);

                    // 检查当前练习是否完成且无错误，才自动跳转
                    if session_model.session.is_current_exercise_complete()
                        && !session_model.session.current_exercise_has_errors()
                    {
                        if session_model.session.has_next_exercise() {
                            session_model.session.advance_to_next_exercise();
                            debug!("✅ 练习无错误，自动跳转到下一个练习");
                        }
                    }
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
        // 检查是否需要重新加载课程
        let should_reload = if let Ok(mut needs_reload) = self.needs_reload.lock() {
            let should = *needs_reload;
            *needs_reload = false;
            should
        } else {
            false
        };

        if should_reload {
            self.reload_lessons(cx);
        }

        // 订阅 session 的变化
        if let Some(session) = &self.session {
            let session_clone = session.clone();
            cx.observe(&session_clone, |_, _, cx| {
                cx.notify();
            })
            .detach();
        }

        let content = if self.show_settings {
            self.render_settings_view(cx)
        } else if let Some(session) = &self.session {
            let is_completed = session.read(cx).is_completed();
            if is_completed {
                // 课程完成时,缓存快照避免 WPM 持续变化
                if self.completion_snapshot.is_none() {
                    self.completion_snapshot = Some(session.read(cx).get_snapshot());
                }
                // 使用缓存的快照 (clone 避免 move)
                let snapshot = self.completion_snapshot.clone().unwrap();
                self.render_completion_stats(snapshot, cx)
            } else {
                self.render_practice_area(cx)
            }
        } else if self.show_history {
            self.render_history_view(cx)
        } else {
            self.render_lesson_list(cx)
        };

        let colors = self.get_colors();

        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .size_full()
            .bg(colors.bg_primary)
            .track_focus(&self.focus_handle)
            .key_context("KeyzenApp")
            .on_action(cx.listener(Self::back_to_list))
            .on_action(cx.listener(Self::show_history))
            .on_action(cx.listener(Self::show_settings))
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
            KeyBinding::new("cmd-,", ShowSettings, Some("KeyzenApp")),
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
            items: vec![
                MenuItem::action("设置", ShowSettings),
                MenuItem::separator(),
                MenuItem::action("退出", Quit),
            ],
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
