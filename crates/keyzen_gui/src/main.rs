use gpui::prelude::*;
use gpui::*;
use keyzen_core::*;
use keyzen_data::LessonLoader;
use keyzen_engine::TypingSession;
use std::sync::mpsc;

// 定义 Actions
actions!(keyzen, [Quit, BackToList]);

struct KeyzenApp {
    session: Option<Model<SessionModel>>,
    lessons: Vec<Lesson>,
    selected_lesson: Option<usize>,
    focus_handle: FocusHandle,
}

struct SessionModel {
    session: TypingSession,
    _event_rx: mpsc::Receiver<TypingEvent>,
}

impl SessionModel {
    fn new(lesson: Lesson, _cx: &mut ModelContext<Self>) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let session = TypingSession::new(lesson, PracticeMode::Zen, Some(event_tx));

        Self {
            session,
            _event_rx: event_rx,
        }
    }

    fn handle_keystroke(&mut self, key: &str, cx: &mut ModelContext<Self>) {
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
}

impl KeyzenApp {
    fn new(cx: &mut ViewContext<Self>) -> Self {
        let loader = LessonLoader::new("./lessons");
        let lessons = loader.load_all().unwrap_or_default();

        Self {
            session: None,
            lessons,
            selected_lesson: None,
            focus_handle: cx.focus_handle(),
        }
    }

    fn start_lesson(&mut self, lesson_index: usize, cx: &mut ViewContext<Self>) {
        if let Some(lesson) = self.lessons.get(lesson_index).cloned() {
            self.session = Some(cx.new_model(|cx| SessionModel::new(lesson, cx)));
            self.selected_lesson = Some(lesson_index);
            cx.focus(&self.focus_handle);
            cx.notify();
        }
    }

    fn back_to_list(&mut self, _: &BackToList, cx: &mut ViewContext<Self>) {
        self.session = None;
        self.selected_lesson = None;
        cx.focus(&self.focus_handle);
        cx.notify();
    }

    fn render_lesson_list(&self, cx: &mut ViewContext<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_6()
            .w(px(600.0))
            .child(
                div()
                    .flex()
                    .justify_center()
                    .text_size(px(20.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(0xF0F0F0))
                    .child("选择课程")
            )
            .children(
                self.lessons.iter().enumerate().map(|(i, lesson)| {
                    let lesson_index = i;
                    div()
                        .p_4()
                        .bg(rgb(0x2A2A2A))
                        .hover(|style| style.bg(rgb(0x3A3A3A)))
                        .rounded(px(12.0))
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _event, cx| {
                            this.start_lesson(lesson_index, cx);
                        }))
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
                                        .child(format!("{}. {}", i + 1, lesson.title))
                                )
                                .child(
                                    div()
                                        .text_size(px(14.0))
                                        .text_color(rgb(0xA0A0A0))
                                        .child(lesson.description.clone())
                                )
                        )
                })
            )
    }

    fn render_practice_area(&self, session: &SessionModel) -> Div {
        let snapshot = session.get_snapshot();
        let target_text = session.get_target_text();
        let input_text = session.get_input_text();
        let target_chars: Vec<char> = target_text.chars().collect();
        let input_chars: Vec<char> = input_text.chars().collect();

        div()
            .flex()
            .flex_col()
            .gap_8()
            .w(px(800.0))
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
                    .child(format!("进度: {:.0}%", snapshot.progress * 100.0))
            )
            .child(
                div()
                    .p_12()
                    .bg(rgb(0x2A2A2A))
                    .rounded(px(16.0))
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_size(px(24.0))
                            .line_height(relative(1.6))
                            .flex()
                            .flex_row()
                            .flex_wrap()
                            .children(
                                target_chars.iter().enumerate().map(|(i, &target_char)| {
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

                                    let display_char = if target_char == ' ' {
                                        '_'
                                    } else {
                                        target_char
                                    };

                                    let mut char_div = div()
                                        .text_color(color)
                                        .child(display_char.to_string());

                                    if let Some(bg) = bg_color {
                                        char_div = char_div.bg(bg);
                                    }

                                    char_div
                                })
                            )
                    )
            )
            .child(
                div()
                    .flex()
                    .justify_center()
                    .text_xs()
                    .text_color(rgb(0x666666))
                    .child("按 Esc 返回课程列表")
            )
    }
}

impl Render for KeyzenApp {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        // 订阅 session 的变化
        if let Some(session) = &self.session {
            let session_clone = session.clone();
            cx.observe(&session_clone, |_, _, cx| {
                cx.notify();
            })
            .detach();
        }

        let content = if let Some(session) = &self.session {
            let session_ref = session.read(cx);
            self.render_practice_area(session_ref)
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
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, cx| {
                if let Some(session) = &this.session {
                    let key = event.keystroke.key.as_str();
                    session.update(cx, |session, cx| {
                        session.handle_keystroke(key, cx);
                    });
                }
            }))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        div()
                            .text_xl()
                            .text_color(rgb(0x00C2B8))
                            .child("KEYZEN - 键禅")
                    )
                    .child(content)
            )
    }
}

fn quit(_: &Quit, cx: &mut AppContext) {
    cx.quit();
}

fn main() {
    App::new().run(|cx: &mut AppContext| {
        // 绑定快捷键
        cx.bind_keys([
            KeyBinding::new("escape", BackToList, Some("KeyzenApp")),
            KeyBinding::new("cmd-q", Quit, None),
        ]);

        // 注册退出动作
        cx.on_action(quit);

        // 创建应用菜单
        cx.set_menus(vec![Menu {
            name: "Keyzen".into(),
            items: vec![
                MenuItem::action("退出", Quit),
            ],
        }]);

        // 打开窗口
        let bounds = Bounds::centered(None, size(px(900.0), px(650.0)), cx);
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Keyzen - 键禅".into()),
                        appears_transparent: false,
                        traffic_light_position: None,
                    }),
                    ..Default::default()
                },
                |cx| cx.new_view(|cx| KeyzenApp::new(cx)),
            )
            .unwrap();

        // 设置焦点并激活应用
        window
            .update(cx, |view, cx| {
                cx.focus(&view.focus_handle);
                cx.activate(true);
            })
            .unwrap();
    });
}
