use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, Paragraph, Wrap},
    Frame, Terminal,
};
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::api::{MiMoClient, StreamEvent};
use crate::config::Config;

#[derive(Debug, Clone)]
pub enum ChatMessage {
    User(String),
    Assistant(String),
    Thinking(String),
    ToolCall { name: String, args_summary: String },
    ToolResult { name: String, result: String },
    Error(String),
    System(String),
}

pub struct App {
    pub config: Config,
    pub messages: Vec<ChatMessage>,
    pub input: String,
    pub cursor_position: usize,
    pub scroll_offset: usize,
    pub is_streaming: bool,
    pub thinking_expanded: bool,
    pub status_msg: String,
    pub should_quit: bool,
    // Streaming buffers (accumulated during streaming)
    pub stream_content: String,
    pub stream_thinking: String,
    // Pending tool results to send back to API
    pub pending_tool_results: Vec<(String, String)>, // (call_id, result)
}

impl App {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            messages: vec![ChatMessage::System(
                format!("  🐋  MiMo TUI v{}\n\n  Ready to code. Type a message or paste code to start.\n\n  /help for commands · Tab: expand thinking · Ctrl+C: quit", env!("CARGO_PKG_VERSION"))
            )],
            input: String::new(),
            cursor_position: 0,
            scroll_offset: 0,
            is_streaming: false,
            thinking_expanded: false,
            status_msg: String::new(),
            should_quit: false,
            stream_content: String::new(),
            stream_thinking: String::new(),
            pending_tool_results: Vec::new(),
        }
    }

    pub fn handle_key(&mut self, key: KeyEvent) -> Option<String> {
        if self.is_streaming {
            if key.code == KeyCode::Esc
                || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
            {
                self.is_streaming = false;
                self.status_msg = "Cancelled".into();
            }
            return None;
        }

        match key.code {
            KeyCode::Enter => {
                if !self.input.trim().is_empty() {
                    let input = self.input.clone();
                    self.messages.push(ChatMessage::User(input.clone()));
                    self.input.clear();
                    self.cursor_position = 0;
                    self.is_streaming = true;
                    self.stream_content.clear();
                    self.stream_thinking.clear();
                    self.status_msg = "streaming...".into();
                    return Some(input);
                }
                None
            }
            KeyCode::Char(c) if key.modifiers.contains(KeyModifiers::CONTROL) => {
                match c {
                    'c' => self.should_quit = true,
                    'l' => { self.messages.clear(); self.scroll_offset = 0; }
                    'k' => { self.input.clear(); self.cursor_position = 0; }
                    _ => {}
                }
                None
            }
            KeyCode::Char(c) => {
                self.input.insert(self.cursor_position, c);
                self.cursor_position += c.len_utf8();
                None
            }
            KeyCode::Backspace => {
                if self.cursor_position > 0 {
                    let new_pos = self.input[..self.cursor_position]
                        .char_indices().last().map(|(i, _)| i).unwrap_or(0);
                    self.input.drain(new_pos..self.cursor_position);
                    self.cursor_position = new_pos;
                }
                None
            }
            KeyCode::Delete => {
                if self.cursor_position < self.input.len() {
                    let next = self.input[self.cursor_position..]
                        .char_indices().nth(1).map(|(i, _)| self.cursor_position + i)
                        .unwrap_or(self.input.len());
                    self.input.drain(self.cursor_position..next);
                }
                None
            }
            KeyCode::Left => {
                if self.cursor_position > 0 {
                    self.cursor_position = self.input[..self.cursor_position]
                        .char_indices().last().map(|(i, _)| i).unwrap_or(0);
                }
                None
            }
            KeyCode::Right => {
                if self.cursor_position < self.input.len() {
                    self.cursor_position = self.input[self.cursor_position..]
                        .char_indices().nth(1).map(|(i, _)| self.cursor_position + i)
                        .unwrap_or(self.input.len());
                }
                None
            }
            KeyCode::Home => { self.cursor_position = 0; None }
            KeyCode::End => { self.cursor_position = self.input.len(); None }
            KeyCode::Up => { self.scroll_offset = self.scroll_offset.saturating_sub(1); None }
            KeyCode::Down => {
                let max = self.messages.len().saturating_sub(5);
                if self.scroll_offset < max { self.scroll_offset += 1; }
                None
            }
            KeyCode::PageUp => { self.scroll_offset = self.scroll_offset.saturating_sub(10); None }
            KeyCode::PageDown => {
                let max = self.messages.len().saturating_sub(5);
                self.scroll_offset = (self.scroll_offset + 10).min(max);
                None
            }
            KeyCode::Tab => { self.thinking_expanded = !self.thinking_expanded; None }
            KeyCode::Esc => { self.is_streaming = false; None }
            _ => None,
        }
    }

    /// Process a stream event from the API
    pub fn on_stream_event(&mut self, event: StreamEvent) {
        match event {
            StreamEvent::Thinking(text) => {
                self.stream_thinking.push_str(&text);
            }
            StreamEvent::Content(text) => {
                // Flush thinking if needed
                if !self.stream_thinking.is_empty() && !self.stream_content.is_empty() {
                    // Thinking already shown inline, don't double-flush
                }
                self.stream_content.push_str(&text);
            }
            StreamEvent::ToolCall { name, id, arguments } => {
                self.flush_stream_buffers();
                let args: serde_json::Value = serde_json::from_str(&arguments).unwrap_or_default();
                let args_summary = format_args_short(&args);
                self.messages.push(ChatMessage::ToolCall { name: name.clone(), args_summary });

                // Execute tool
                let result = crate::tools::execute_tool(&name, &args);
                self.messages.push(ChatMessage::ToolResult { name, result: result.clone() });

                // Store for sending back to API
                self.pending_tool_results.push((id, result));
                self.scroll_to_end();
            }
            StreamEvent::Error(msg) => {
                self.flush_stream_buffers();
                self.messages.push(ChatMessage::Error(msg));
                self.scroll_to_end();
            }
            StreamEvent::AssistantMessage(_msg) => {
                // Synced back to main client separately
            }
            StreamEvent::Done => {
                self.flush_stream_buffers();
                self.is_streaming = false;
                self.scroll_to_end();
            }
        }
    }

    fn flush_stream_buffers(&mut self) {
        if !self.stream_thinking.is_empty() {
            self.messages.push(ChatMessage::Thinking(self.stream_thinking.clone()));
            self.stream_thinking.clear();
        }
        if !self.stream_content.is_empty() {
            self.messages.push(ChatMessage::Assistant(self.stream_content.clone()));
            self.stream_content.clear();
        }
    }

    fn scroll_to_end(&mut self) {
        self.scroll_offset = self.messages.len().saturating_sub(1);
    }

    pub fn update_status(&mut self, _model: &str, input: u64, output: u64, reasoning: u64) {
        self.status_msg = format!("↑{} ↓{} 💭{}", input, output, reasoning);
    }
}

// ─── Rendering ───────────────────────────────────────────────────

pub fn ui(f: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(3),
            Constraint::Length(3),
            Constraint::Length(1),
        ])
        .split(f.area());

    render_chat(f, app, chunks[0]);
    render_input(f, app, chunks[1]);
    render_status(f, app, chunks[2]);
}

fn render_chat(f: &mut Frame, app: &App, area: Rect) {
    let mut lines: Vec<Line> = Vec::new();

    for msg in &app.messages {
        render_message(&mut lines, msg, app.thinking_expanded);
    }

    // Live streaming content
    if app.is_streaming {
        if !app.stream_thinking.is_empty() {
            if app.thinking_expanded {
                lines.push(Line::from(Span::styled("💭 Thinking:", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))));
                for line in app.stream_thinking.lines() {
                    lines.push(Line::from(Span::styled(format!("  {}", line), Style::default().fg(Color::DarkGray))));
                }
            } else {
                let preview = safe_preview(&app.stream_thinking, 80);
                lines.push(Line::from(Span::styled(format!("💭 {}", preview), Style::default().fg(Color::DarkGray))));
            }
        }
        if !app.stream_content.is_empty() {
            lines.push(Line::from(Span::styled("🐋 MiMo:", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))));
            for line in app.stream_content.lines() {
                lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::White))));
            }
            lines.push(Line::from(Span::styled("▌", Style::default().fg(Color::Cyan)))); // cursor
        } else if app.stream_thinking.is_empty() {
            lines.push(Line::from(Span::styled("  ⏳ waiting...", Style::default().fg(Color::DarkGray))));
        }
    }

    let visible_height = area.height as usize;
    let total = lines.len();
    let max_scroll = total.saturating_sub(visible_height);
    let scroll = app.scroll_offset.min(max_scroll);
    let visible: Vec<Line> = lines.into_iter().skip(scroll).take(visible_height).collect();

    let block = Paragraph::new(Text::from(visible))
        .block(Block::default().borders(Borders::ALL)
            .title(" MiMo TUI ")
            .title_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)))
        .wrap(Wrap { trim: false });

    f.render_widget(block, area);
}

fn render_message(lines: &mut Vec<Line>, msg: &ChatMessage, thinking_expanded: bool) {
    match msg {
        ChatMessage::User(text) => {
            lines.push(Line::from(vec![
                Span::styled("❯ ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
                Span::styled(text.clone(), Style::default().fg(Color::White)),
            ]));
            lines.push(Line::from(""));
        }
        ChatMessage::Assistant(text) => {
            lines.push(Line::from(Span::styled("🐋 MiMo:", Style::default().fg(Color::Magenta).add_modifier(Modifier::BOLD))));
            for line in text.lines() {
                if line.starts_with("```") {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::DarkGray))));
                } else if line.starts_with("**") || line.starts_with("## ") {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD))));
                } else if line.starts_with("- ") || line.starts_with("* ") {
                    lines.push(Line::from(vec![
                        Span::styled("  • ", Style::default().fg(Color::DarkGray)),
                        Span::styled(line[2..].to_string(), Style::default().fg(Color::White)),
                    ]));
                } else if line.starts_with("| ") && line.contains(" | ") {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::Green))));
                } else {
                    lines.push(Line::from(Span::styled(line.to_string(), Style::default().fg(Color::White))));
                }
            }
            lines.push(Line::from(""));
        }
        ChatMessage::Thinking(text) => {
            if thinking_expanded {
                lines.push(Line::from(Span::styled("💭 Thinking:", Style::default().fg(Color::DarkGray).add_modifier(Modifier::ITALIC))));
                for line in text.lines() {
                    lines.push(Line::from(Span::styled(format!("  {}", line), Style::default().fg(Color::DarkGray))));
                }
            } else {
                lines.push(Line::from(Span::styled(
                    format!("💭 {}", safe_preview(text, 80)),
                    Style::default().fg(Color::DarkGray),
                )));
            }
            lines.push(Line::from(""));
        }
        ChatMessage::ToolCall { name, args_summary } => {
            lines.push(Line::from(vec![
                Span::styled("  ⚡ ", Style::default().fg(Color::Yellow)),
                Span::styled(name.clone(), Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled(format!("({})", args_summary), Style::default().fg(Color::DarkGray)),
            ]));
        }
        ChatMessage::ToolResult { name, result } => {
            lines.push(Line::from(Span::styled(format!("  ├─ {} result:", name), Style::default().fg(Color::Green))));
            let preview_lines: Vec<&str> = result.lines().take(8).collect();
            for line in &preview_lines {
                lines.push(Line::from(Span::styled(format!("  │  {}", line), Style::default().fg(Color::DarkGray))));
            }
            let total = result.lines().count();
            if total > 8 {
                lines.push(Line::from(Span::styled(format!("  └─ ... ({} lines)", total), Style::default().fg(Color::DarkGray))));
            } else {
                lines.push(Line::from(Span::styled("  └─", Style::default().fg(Color::DarkGray))));
            }
            lines.push(Line::from(""));
        }
        ChatMessage::Error(msg) => {
            lines.push(Line::from(Span::styled(format!("❌ {}", msg), Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))));
            lines.push(Line::from(""));
        }
        ChatMessage::System(msg) => {
            lines.push(Line::from(Span::styled(format!("ℹ {}", msg), Style::default().fg(Color::Blue))));
            lines.push(Line::from(""));
        }
    }
}

fn render_input(f: &mut Frame, app: &App, area: Rect) {
    let (prompt, style) = match app.config.permission_mode.as_str() {
        "plan" => ("📋❯ ", Style::default().fg(Color::Blue)),
        "yolo" => ("🔥❯ ", Style::default().fg(Color::Red)),
        _ => ("❯ ", Style::default().fg(Color::Green)),
    };

    let text = format!("{}{}", prompt, app.input);
    let block = Paragraph::new(text).style(style)
        .block(Block::default().borders(Borders::ALL)
            .title(" Input ")
            .title_style(Style::default().fg(Color::DarkGray)));

    f.render_widget(block, area);

    let prompt_w = unicode_width(prompt);
    let cursor_w = if app.cursor_position <= app.input.len() { app.input[..app.cursor_position].chars().count() } else { 0 };
    f.set_cursor_position((area.x + prompt_w as u16 + cursor_w as u16 + 1, area.y + 1));
}

fn render_status(f: &mut Frame, app: &App, area: Rect) {
    let mode = match app.config.permission_mode.as_str() {
        "plan" => "📋 PLAN",
        "yolo" => "🔥 YOLO",
        _ => "🤖 AGENT",
    };
    let status = format!(" {} │ {} │ {} │ Tab:think │ Ctrl+C:quit", app.config.model, mode, app.status_msg);
    let style = if app.is_streaming {
        Style::default().fg(Color::Black).bg(Color::Cyan)
    } else {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    };
    f.render_widget(Paragraph::new(status).style(style), area);
}

fn unicode_width(s: &str) -> usize {
    s.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum()
}

fn safe_preview(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let end = s.char_indices().nth(max_chars).map(|(i, _)| i).unwrap_or(s.len());
    format!("{}...", &s[..end])
}

fn format_args_short(args: &serde_json::Value) -> String {
    args.as_object()
        .map(|m| {
            m.iter().take(3).map(|(k, v)| {
                let default = v.to_string();
                let val = v.as_str().unwrap_or(&default);
                let end = val.char_indices().nth(40).map(|(i, _)| i).unwrap_or(val.len());
                let short = if val.len() > 40 { format!("{}...", &val[..end]) } else { val.to_string() };
                format!("{}={}", k, short)
            }).collect::<Vec<_>>().join(", ")
        })
        .unwrap_or_default()
}

// ─── Terminal Cleanup Guard ──────────────────────────────────────

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        use crossterm::terminal::{disable_raw_mode, LeaveAlternateScreen};
        use crossterm::event::DisableMouseCapture;
        let _ = disable_raw_mode();
        let _ = crossterm::execute!(io::stdout(), LeaveAlternateScreen, DisableMouseCapture);
    }
}

// ─── TUI Runner ──────────────────────────────────────────────────

pub async fn run_tui(config: Config) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let _guard = TerminalGuard; // Ensures cleanup on any exit path
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(config.clone());
    let mut client = MiMoClient::new(config);
    client.add_system_prompt();

    // Channel for streaming events
    let (tx, mut rx) = mpsc::channel::<StreamEvent>(256);

    loop {
        terminal.draw(|f| ui(f, &app))?;

        // Check for streaming events
        while let Ok(event) = rx.try_recv() {
            // Sync assistant message back to main client
            if let StreamEvent::AssistantMessage(ref msg) = event {
                client.messages.push(msg.clone());
            }
            app.on_stream_event(event);
            // Re-draw after each event for real-time updates
            terminal.draw(|f| ui(f, &app))?;
        }

        // Poll for keyboard events
        if event::poll(Duration::from_millis(16))? {
            if let Event::Key(key) = event::read()? {
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) && !app.is_streaming {
                    break;
                }
                if let Some(user_input) = app.handle_key(key) {
                    // User submitted input — spawn streaming task
                    let tx = tx.clone();
                    let config = client.config.clone();
                    let messages = client.messages.clone();

                    tokio::spawn(async move {
                        let mut temp_client = MiMoClient::new(config);
                        temp_client.messages = messages;
                        temp_client.add_user_message(user_input);

                        let (events, result) = match temp_client.stream_turn().await {
                            Ok(r) => r,
                            Err(e) => {
                                let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                                let _ = tx.send(StreamEvent::Done).await;
                                return;
                            }
                        };

                        for event in events {
                            // Insert assistant message sync before Done
                            if matches!(event, StreamEvent::Done) {
                                let _ = tx.send(StreamEvent::AssistantMessage(result.assistant_message.clone())).await;
                            }
                            if tx.send(event).await.is_err() {
                                break;
                            }
                        }
                    });
                }
            }
        }

        // If streaming just finished and there are pending tool results,
        // we need to feed them back and continue the agent loop
        if !app.is_streaming && !app.pending_tool_results.is_empty() {
            let results: Vec<(String, String)> = app.pending_tool_results.drain(..).collect();
            for (call_id, result) in results {
                client.add_tool_result(call_id, result);
            }

            // Continue the agent loop
            app.is_streaming = true;
            app.stream_content.clear();
            app.stream_thinking.clear();

            let tx = tx.clone();
            let config = client.config.clone();
            let messages = client.messages.clone();

            tokio::spawn(async move {
                let mut temp_client = MiMoClient::new(config);
                temp_client.messages = messages;

                let (events, result) = match temp_client.stream_turn().await {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(StreamEvent::Error(e.to_string())).await;
                        let _ = tx.send(StreamEvent::Done).await;
                        return;
                    }
                };

                for event in events {
                    if matches!(event, StreamEvent::Done) {
                        let _ = tx.send(StreamEvent::AssistantMessage(result.assistant_message.clone())).await;
                    }
                    if tx.send(event).await.is_err() { break; }
                }
            });
        }

        // Sync client state from the temp client
        // This is a simplified approach — in production you'd want proper state sync
        app.update_status(&client.config.model, client.total_input_tokens, client.total_output_tokens, client.total_reasoning_tokens);

        if app.should_quit {
            break;
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    terminal.show_cursor()?;
    Ok(())
}
