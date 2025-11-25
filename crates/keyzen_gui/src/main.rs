use gpui::prelude::*;
use gpui::*;
use keyzen_core::*;
use keyzen_data::LessonLoader;
use keyzen_engine::TypingSession;
use std::sync::mpsc;

struct KeyzenApp {
    session: Option<Model<SessionModel>>,
    lessons: Vec<Lesson>,
    selected_lesson: Option<usize>,
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

    fn handle_keystroke(&mut self, key: &Keystroke, cx: &mut ModelContext<Self>) {
        // 处理按键
        if let Some(ch) = key.key.chars().next() {
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
    fn new(_cx: &mut AppContext) -> Self {
        // 加载课程
        let loader = LessonLoader::new("./lessons");
        let lessons = loader.load_all().unwrap_or_default();

        Self {
            session: None,
            lessons,
            selected_lesson: None,
        }
    }

    fn start_lesson(&mut self, lesson_index: usize, cx: &mut AppContext) {
        if let Some(lesson) = self.lessons.get(lesson_index).cloned() {
            self.session = Some(cx.new_model(|cx| SessionModel::new(lesson, cx)));
            self.selected_lesson = Some(lesson_index);
        }
    }
}

impl Render for KeyzenApp {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        let content = if let Some(session_model) = &self.session {
            let session_ref = session_model.read(cx);
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
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_4()
                    .child(
                        // Logo
                        div()
                            .text_xl()
                            .text_color(rgb(0x00C2B8))
                            .child("KEYZEN - 键禅")
                    )
                    .child(content)
            )
    }
}

impl KeyzenApp {
    fn render_lesson_list(&self, cx: &mut ViewContext<Self>) -> Div {
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_lg()
                    .text_color(rgb(0xF0F0F0))
                    .child("选择课程：")
            )
            .children(
                self.lessons.iter().enumerate().map(|(i, lesson)| {
                    let lesson_index = i;
                    div()
                        .p_2()
                        .bg(rgb(0x2A2A2A))
                        .hover(|style| style.bg(rgb(0x3A3A3A)))
                        .rounded_md()
                        .cursor_pointer()
                        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _event, cx| {
                            this.start_lesson(lesson_index, cx);
                        }))
                        .child(
                            div()
                                .text_color(rgb(0xF0F0F0))
                                .child(format!("{}. {}", i + 1, lesson.title))
                        )
                })
            )
    }

    fn render_practice_area(
        &self,
        session: &SessionModel,
    ) -> Div {
        let snapshot = session.get_snapshot();
        let target_text = session.get_target_text();
        let input_text = session.get_input_text();
        let target_chars: Vec<char> = target_text.chars().collect();
        let input_chars: Vec<char> = input_text.chars().collect();

        div()
            .flex()
            .flex_col()
            .gap_4()
            .child(
                // 统计信息
                div()
                    .text_sm()
                    .text_color(rgb(0xA0A0A0))
                    .child(format!(
                        "WPM: {:.0}  |  准确率: {:.1}%  |  进度: {:.0}%",
                        snapshot.current_wpm,
                        snapshot.accuracy * 100.0,
                        snapshot.progress * 100.0
                    ))
            )
            .child(
                // 练习文本区
                div()
                    .p_4()
                    .bg(rgb(0x2A2A2A))
                    .rounded_md()
                    .min_w(px(600.0))
                    .child(
                        div()
                            .font_family("JetBrains Mono")
                            .text_2xl()
                            .flex()
                            .flex_row()
                            .children(
                                target_chars.iter().enumerate().map(|(i, &target_char)| {
                                    let color = if i < input_chars.len() {
                                        let input_char = input_chars[i];
                                        if input_char == target_char {
                                            rgb(0xF0F0F0) // 正确 - 亮白
                                        } else {
                                            rgb(0xFF9966) // 错误 - 橘红
                                        }
                                    } else if i == input_chars.len() {
                                        rgb(0x00C2B8) // 当前位置 - 青色
                                    } else {
                                        rgb(0xA0A0A0) // 未输入 - 灰色
                                    };

                                    let display_char = if i < input_chars.len() {
                                        input_chars[i]
                                    } else {
                                        target_char
                                    };

                                    div()
                                        .text_color(color)
                                        .when(i == input_chars.len(), |style| {
                                            style.bg(rgb(0x00C2B8)).text_color(rgb(0x000000))
                                        })
                                        .child(display_char.to_string())
                                })
                            )
                    )
            )
            .child(
                // 提示
                div()
                    .text_sm()
                    .text_color(rgb(0x666666))
                    .child("按 Esc 退出")
            )
    }
}

actions!(keyzen, [Quit]);

fn main() {
    App::new().run(|cx: &mut AppContext| {
        cx.activate(true);
        cx.on_action(|_action: &Quit, cx| cx.quit());

        cx.bind_keys([KeyBinding::new("escape", Quit, None)]);

        // 居中显示窗口
        let window_options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None, // 自动检测主屏幕
                size(px(900.0), px(650.0)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("Keyzen - 键禅".into()),
                appears_transparent: false,
                traffic_light_position: None,
            }),
            ..Default::default()
        };

        cx.open_window(window_options, |cx| {
            let app = KeyzenApp::new(cx);
            cx.new_view(|_cx| app)
        })
        .unwrap();
    });
}
