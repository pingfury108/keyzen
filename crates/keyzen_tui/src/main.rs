use anyhow::Result;
use keyzen_core::*;
use keyzen_data::LessonLoader;
use keyzen_engine::TypingSession;
use ratatui::{
    backend::CrosstermBackend,
    crossterm::{
        event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
        execute,
        terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    },
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io;
use std::sync::mpsc;

struct App {
    session: TypingSession,
    event_rx: mpsc::Receiver<TypingEvent>,
    completed: bool,
    final_stats: Option<SessionStats>,
}

impl App {
    fn new(lesson: Lesson) -> Self {
        let (event_tx, event_rx) = mpsc::channel();
        let session = TypingSession::new(lesson, PracticeMode::Zen, Some(event_tx));

        Self {
            session,
            event_rx,
            completed: false,
            final_stats: None,
        }
    }

    fn handle_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                return true; // 退出
            }
            KeyCode::Esc => {
                return true; // 退出
            }
            KeyCode::Char(c) => {
                self.session.handle_keystroke(c);
            }
            KeyCode::Backspace => {
                self.session.handle_keystroke('\u{0008}');
            }
            KeyCode::Enter => {
                self.session.handle_keystroke('\n');
            }
            KeyCode::Tab => {
                self.session.handle_keystroke('\t');
            }
            _ => {}
        }

        // 处理事件
        while let Ok(event) = self.event_rx.try_recv() {
            if let TypingEvent::SessionCompleted { stats } = event {
                self.completed = true;
                self.final_stats = Some(stats);
            }
        }

        false
    }

    fn render(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(2)
            .constraints([
                Constraint::Length(3), // Logo
                Constraint::Length(3), // 统计信息
                Constraint::Min(10),   // 核心练习区
                Constraint::Length(3), // 提示信息
            ])
            .split(frame.area());

        // Logo
        let logo = Paragraph::new("KEYZEN - 键禅")
            .style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(logo, chunks[0]);

        if self.completed {
            self.render_completion(frame, chunks[2]);
        } else {
            self.render_practice(frame, chunks);
        }
    }

    fn render_practice(&self, frame: &mut Frame, chunks: std::rc::Rc<[ratatui::layout::Rect]>) {
        let snapshot = self.session.get_snapshot();

        // 统计信息
        let stats_text = format!(
            "WPM: {:.0}  |  准确率: {:.1}%  |  进度: {:.0}%",
            snapshot.current_wpm,
            snapshot.accuracy * 100.0,
            snapshot.progress * 100.0
        );
        let stats = Paragraph::new(stats_text)
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(stats, chunks[1]);

        // 核心练习区
        let target_text = self.session.get_target_text();
        let input_text = self.session.get_input_text();
        let target_chars: Vec<char> = target_text.chars().collect();
        let input_chars: Vec<char> = input_text.chars().collect();

        let mut spans = Vec::new();
        for (i, &target_char) in target_chars.iter().enumerate() {
            let style = if i < input_chars.len() {
                // 已输入
                let input_char = input_chars[i];
                if input_char == target_char {
                    // 正确
                    Style::default().fg(Color::White)
                } else {
                    // 错误
                    Style::default()
                        .fg(Color::Red)
                        .add_modifier(Modifier::UNDERLINED)
                }
            } else if i == input_chars.len() {
                // 当前位置（光标）
                Style::default()
                    .fg(Color::Black)
                    .bg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            } else {
                // 未输入
                Style::default().fg(Color::DarkGray)
            };

            let display_char = if i < input_chars.len() {
                if input_chars[i] == target_char {
                    target_char
                } else {
                    input_chars[i]
                }
            } else {
                target_char
            };

            spans.push(Span::styled(display_char.to_string(), style));
        }

        let practice_area = Paragraph::new(Line::from(spans))
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("练习区")
                    .title_alignment(Alignment::Center),
            )
            .wrap(Wrap { trim: false })
            .alignment(Alignment::Left);
        frame.render_widget(practice_area, chunks[2]);

        // 提示信息
        let help = Paragraph::new("按 Esc 或 Ctrl+C 退出")
            .style(Style::default().fg(Color::DarkGray))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(help, chunks[3]);
    }

    fn render_completion(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        if let Some(stats) = &self.final_stats {
            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(
                    "🎉 课程完成！",
                    Style::default()
                        .fg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(format!("最终速度：  {:.0} WPM", stats.wpm)),
                Line::from(format!("准确率：    {:.1}%", stats.accuracy * 100.0)),
                Line::from(format!("用时：      {:.0}秒", stats.duration.as_secs())),
                Line::from(format!("总按键数：  {}", stats.total_keystrokes)),
                Line::from(format!("错误数：    {}", stats.error_count)),
                Line::from(""),
            ];

            if !stats.weak_keys.is_empty() {
                lines.push(Line::from(Span::styled(
                    "薄弱按键：",
                    Style::default().fg(Color::Yellow),
                )));
                for (ch, rate) in &stats.weak_keys {
                    lines.push(Line::from(format!(
                        "  '{}' → 错误率 {:.1}%",
                        ch,
                        rate * 100.0
                    )));
                }
                lines.push(Line::from(""));
            }

            lines.push(Line::from(Span::styled(
                "按 Esc 退出",
                Style::default().fg(Color::DarkGray),
            )));

            let completion = Paragraph::new(lines).alignment(Alignment::Center).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("完成")
                    .title_alignment(Alignment::Center),
            );
            frame.render_widget(completion, area);
        }
    }
}

fn run_app(lesson: Lesson) -> Result<()> {
    // 设置终端
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 创建应用
    let mut app = App::new(lesson);

    // 主循环
    loop {
        terminal.draw(|f| app.render(f))?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key_event) = event::read()? {
                if app.handle_key(key_event) {
                    break;
                }
            }
        }
    }

    // 恢复终端
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn main() -> Result<()> {
    // 加载课程
    let loader = LessonLoader::new("./lessons");
    let lessons = loader.load_all()?;

    if lessons.is_empty() {
        println!("未找到课程文件，请先创建课程。");
        println!("课程文件应放在 ./lessons/ 目录下，格式为 .ron");
        return Ok(());
    }

    // 显示课程列表
    println!("\n╔═══════════════════════════════════════╗");
    println!("║         KEYZEN - 键禅                 ║");
    println!("╚═══════════════════════════════════════╝\n");
    println!("可用课程：\n");

    for (i, lesson) in lessons.iter().enumerate() {
        println!("  {}. {} - {}", i + 1, lesson.title, lesson.description);
    }

    println!("\n请输入课程编号（1-{}）：", lessons.len());

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice: usize = input.trim().parse().unwrap_or(1);

    if choice < 1 || choice > lessons.len() {
        println!("无效的选择");
        return Ok(());
    }

    let lesson = lessons[choice - 1].clone();

    // 启动 TUI
    run_app(lesson)?;

    Ok(())
}
