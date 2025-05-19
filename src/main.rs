use crossterm::{
    cursor::Show,
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Frame, Terminal,
};
use rusqlite::Connection;
use std::fs;
use std::io::{self, stdout};
use std::path::Path;
use std::process::Command;
use thiserror::Error;
use tui_textarea::{Input, Key, TextArea};

#[derive(Error, Debug)]
pub enum EditorError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File not found in database: {0}")]
    FileNotFound(String),
    #[error("Markdown scanner error: {0}")]
    Scanner(String),
}

struct App {
    db: Connection,
    file_path: String,
    base_dir: String,
    textarea: TextArea<'static>,
    mode: Mode,
    tags: Vec<String>,
    backlinks: Vec<(String, i64)>,
    view: View,
    command: String,
    status: String,
    file_id: i64,
    should_quit: bool,
    history: Vec<(String, i64)>, // (file_path, file_id)
    history_index: usize,        // Current position in history
}

#[derive(PartialEq)]
enum Mode {
    Normal,
    Insert,
    Command,
}

#[derive(PartialEq)]
enum View {
    Editor,
    Info,
}

impl App {
    fn new(file_path: &str, base_dir: &str) -> Result<Self, EditorError> {
        let db = Connection::open("markdown_data.db")?;
        db.execute("PRAGMA foreign_keys = ON;", [])?;

        let content = fs::read_to_string(file_path).unwrap_or_default();
        let mut textarea = TextArea::new(content.lines().map(|s| s.to_string()).collect());
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Markdown Editor")
                .style(Style::default().fg(Color::White)),
        );
        textarea.set_cursor_line_style(Style::default());
        textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));

        let file_id = App::get_file_id(&db, file_path)?;
        let tags = App::load_tags(&db, file_id)?;
        let backlinks = App::load_backlinks(&db, file_id)?;

        Ok(App {
            db,
            file_path: file_path.to_string(),
            base_dir: base_dir.to_string(),
            textarea,
            mode: Mode::Normal,
            tags,
            backlinks,
            view: View::Editor,
            command: String::new(),
            status: "Normal".to_string(),
            file_id,
            should_quit: false,
            history: vec![(file_path.to_string(), file_id)],
            history_index: 0,
        })
    }

    fn get_file_id(db: &Connection, path: &str) -> Result<i64, EditorError> {
        let mut stmt = db.prepare("SELECT id FROM files WHERE path = ?")?;
        stmt.query_row([path], |row| row.get(0))
            .map_err(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => EditorError::FileNotFound(path.to_string()),
                e => EditorError::Database(e),
            })
    }

    fn load_tags(db: &Connection, file_id: i64) -> Result<Vec<String>, EditorError> {
        let mut stmt = db.prepare(
            "SELECT t.tag FROM tags t
             JOIN file_tags ft ON t.id = ft.tag_id
             WHERE ft.file_id = ?",
        )?;
        let tags = stmt
            .query_map([file_id], |row| row.get(0))?
            .collect::<Result<Vec<String>, _>>()?;
        Ok(tags)
    }

    fn load_backlinks(db: &Connection, file_id: i64) -> Result<Vec<(String, i64)>, EditorError> {
        let mut stmt = db.prepare(
            "SELECT DISTINCT b.backlink, f.id
             FROM backlinks b
             JOIN files f ON b.backlink_id = f.id
             WHERE b.file_id = ?",
        )?;
        let mut backlinks = Vec::new();
        let rows = stmt.query_map([file_id], |row| {
            let backlink: String = row.get(0)?;
            let backlink_id: i64 = row.get(1)?;
            Ok((backlink, backlink_id))
        })?;

        for row in rows {
            match row {
                Ok((backlink, backlink_id)) => {
                    backlinks.push((backlink, backlink_id));
                }
                Err(e) => {
                    eprintln!("Error loading backlink: {}", e);
                }
            }
        }

        // Handle ambiguous backlinks by selecting the file with the shortest basename
        let mut unique_backlinks = Vec::new();
        let mut seen_backlinks = std::collections::HashSet::new();
        for (backlink, backlink_id) in backlinks {
            if seen_backlinks.insert(backlink.clone()) {
                unique_backlinks.push((backlink, backlink_id));
            } else {
                let existing = unique_backlinks
                    .iter_mut()
                    .find(|(b, _)| b == &backlink)
                    .expect("Backlink should exist");
                let existing_path = db
                    .query_row("SELECT path FROM files WHERE id = ?", [existing.1], |row| {
                        row.get::<_, String>(0)
                    })
                    .unwrap_or_default();
                let new_path = db
                    .query_row(
                        "SELECT path FROM files WHERE id = ?",
                        [backlink_id],
                        |row| row.get::<_, String>(0),
                    )
                    .unwrap_or_default();

                let existing_basename = Path::new(&existing_path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let new_basename = Path::new(&new_path)
                    .file_name()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();

                if new_basename.len() < existing_basename.len() {
                    *existing = (backlink, backlink_id);
                }
            }
        }

        Ok(unique_backlinks)
    }

    fn save_file(&mut self) -> Result<(), EditorError> {
        fs::write(&self.file_path, self.textarea.lines().join("\n"))?;

        let output = Command::new("markdown-scanner")
            .arg(&self.file_path)
            .arg(&self.base_dir)
            .output()?;

        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(EditorError::Scanner(error_msg));
        }

        self.status = "Saved".to_string();
        Ok(())
    }

    fn open_file(&mut self, path: String, file_id: i64) -> Result<(), EditorError> {
        self.file_path = path.clone();
        self.file_id = file_id;
        let content = fs::read_to_string(&self.file_path).unwrap_or_default();
        self.textarea = TextArea::new(content.lines().map(|s| s.to_string()).collect());
        self.textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Markdown Editor")
                .style(Style::default().fg(Color::White)),
        );
        self.textarea.set_cursor_line_style(Style::default());
        self.textarea
            .set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        self.tags = App::load_tags(&self.db, self.file_id)?;
        self.backlinks = App::load_backlinks(&self.db, self.file_id)?;
        self.view = View::Editor;
        self.status = format!("Opened {}", self.file_path);
        Ok(())
    }

    fn follow_backlink(&mut self, index: usize) -> Result<(), EditorError> {
        if index < self.backlinks.len() {
            let backlink_id = self.backlinks[index].1;
            let mut stmt = self.db.prepare("SELECT path FROM files WHERE id = ?")?;
            let path: String = stmt.query_row([backlink_id], |row| row.get(0))?;
            drop(stmt); // Explicitly drop stmt to end the immutable borrow

            // Update history: truncate future entries and append new file
            self.history.truncate(self.history_index + 1);
            self.history.push((path.clone(), backlink_id));
            self.history_index += 1;

            self.open_file(path, backlink_id)?;
        }
        Ok(())
    }

    fn navigate_back(&mut self) -> Result<(), EditorError> {
        if self.history_index > 0 {
            self.history_index -= 1;
            let (path, file_id) = self.history[self.history_index].clone();
            self.open_file(path, file_id)?;
        } else {
            self.status = "No previous file in history".to_string();
        }
        Ok(())
    }

    fn navigate_forward(&mut self) -> Result<(), EditorError> {
        if self.history_index < self.history.len() - 1 {
            self.history_index += 1;
            let (path, file_id) = self.history[self.history_index].clone();
            self.open_file(path, file_id)?;
        } else {
            self.status = "No next file in history".to_string();
        }
        Ok(())
    }

    fn show_tag_files(&mut self, tag: &str) -> Result<(), EditorError> {
        let mut stmt = self.db.prepare(
            "SELECT f.path FROM files f
             JOIN file_tags ft ON f.id = ft.file_id
             JOIN tags t ON ft.tag_id = t.id
             WHERE t.tag = ?",
        )?;
        let paths = stmt
            .query_map([tag], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        self.status = format!("Files with tag '{}': {}", tag, paths.join(", "));
        self.view = View::Info;
        Ok(())
    }

    fn handle_input(
        &mut self,
        event: ratatui::crossterm::event::KeyEvent,
    ) -> Result<(), EditorError> {
        match self.mode {
            Mode::Normal => {
                let input = Input::from(event);
                match input {
                    Input {
                        key: Key::Char('o'),
                        ctrl: true,
                        ..
                    } => {
                        self.navigate_back()?;
                    }
                    Input {
                        key: Key::Char('i'),
                        ctrl: true,
                        ..
                    } => {
                        self.navigate_forward()?;
                    }
                    Input {
                        key: Key::Char('r'),
                        ctrl: true,
                        ..
                    } => {
                        if self.textarea.redo() {
                            self.status = "Redone".to_string();
                        } else {
                            self.status = "Nothing to redo".to_string();
                        }
                    }
                    Input {
                        key: Key::Char('u'),
                        ..
                    } => {
                        if self.textarea.undo() {
                            self.status = "Undone".to_string();
                        } else {
                            self.status = "Nothing to undo".to_string();
                        }
                    }
                    Input {
                        key: Key::Char('i'),
                        ..
                    } => {
                        self.mode = Mode::Insert;
                        self.status = "Insert".to_string();
                    }
                    Input {
                        key: Key::Char(':'),
                        ..
                    } => {
                        self.mode = Mode::Command;
                        self.command.clear();
                        self.status = "Command".to_string();
                    }
                    Input {
                        key: Key::Char('j'),
                        ..
                    } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Down);
                    }
                    Input {
                        key: Key::Char('k'),
                        ..
                    } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Up);
                    }
                    Input {
                        key: Key::Char('h'),
                        ..
                    } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Back);
                    }
                    Input {
                        key: Key::Char('l'),
                        ..
                    } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Forward);
                    }
                    Input { key: Key::Up, .. } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Up);
                    }
                    Input { key: Key::Down, .. } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Down);
                    }
                    Input { key: Key::Left, .. } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Back);
                    }
                    Input {
                        key: Key::Right, ..
                    } => {
                        self.textarea.move_cursor(tui_textarea::CursorMove::Forward);
                    }
                    Input {
                        key: Key::Enter, ..
                    } => {
                        if self.view == View::Editor {
                            let line = self.textarea.lines()[self.textarea.cursor().0].clone();
                            if let Some(index) = self
                                .backlinks
                                .iter()
                                .position(|(text, _)| line.contains(text))
                            {
                                self.follow_backlink(index)?;
                            } else if let Some(tag) =
                                self.tags.iter().find(|tag| line.contains(&**tag)).cloned()
                            {
                                self.show_tag_files(&tag)?;
                            }
                        } else {
                            self.view = View::Editor;
                            self.status = "Normal".to_string();
                        }
                    }
                    _ => {}
                }
            }
            Mode::Insert => match event.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.status = "Normal".to_string();
                }
                _ => {
                    let input = Input::from(event);
                    self.textarea.input(input);
                }
            },
            Mode::Command => match event.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    self.mode = Mode::Normal;
                    self.status = "Normal".to_string();
                    self.command.clear();
                }
                ratatui::crossterm::event::KeyCode::Enter => {
                    match self.command.as_str() {
                        "w" => self.save_file()?,
                        "q" => self.should_quit = true,
                        "wq" => {
                            self.save_file()?;
                            self.should_quit = true;
                        }
                        _ => self.status = format!("Unknown command: {}", self.command),
                    }
                    self.mode = Mode::Normal;
                    self.command.clear();
                }
                ratatui::crossterm::event::KeyCode::Char(c) => {
                    self.command.push(c);
                }
                ratatui::crossterm::event::KeyCode::Backspace => {
                    self.command.pop();
                }
                _ => {}
            },
        }
        Ok(())
    }

    fn render(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), EditorError> {
        terminal.draw(|f| self.draw(f))?;
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Editor or info area
                Constraint::Length(1), // Status line
                Constraint::Length(1), // Command line
            ])
            .split(f.area());

        match self.view {
            View::Editor => {
                f.render_widget(&self.textarea, chunks[0]);
            }
            View::Info => {
                let info = Paragraph::new(self.status.clone())
                    .block(Block::default().borders(Borders::ALL).title("Info"))
                    .style(Style::default().fg(Color::White));
                f.render_widget(info, chunks[0]);
            }
        }

        let status = Paragraph::new(format!("-- {} --", self.status))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(status, chunks[1]);

        let command = Paragraph::new(if self.mode == Mode::Command {
            format!(":{}", self.command)
        } else {
            String::new()
        })
        .style(Style::default().fg(Color::White));
        f.render_widget(command, chunks[2]);
    }
}

fn main() -> Result<(), EditorError> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <file_path> <base_dir>", args[0]);
        return Ok(());
    }

    // Ensure terminal cleanup on exit
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(stdout(), LeaveAlternateScreen, Show);
            // Run stty echo to ensure terminal echo is restored
            let _ = Command::new("stty").arg("echo").status();
        }
    }
    let _guard = TerminalGuard;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Show)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(&args[1], &args[2])?;

    while !app.should_quit {
        app.render(&mut terminal)?;
        if let Event::Key(event) = event::read()? {
            // Convert crossterm::event::KeyEvent to ratatui::crossterm::event::KeyEvent
            let ratatui_event = ratatui::crossterm::event::KeyEvent {
                code: match event.code {
                    crossterm::event::KeyCode::Char(c) => {
                        ratatui::crossterm::event::KeyCode::Char(c)
                    }
                    crossterm::event::KeyCode::Enter => ratatui::crossterm::event::KeyCode::Enter,
                    crossterm::event::KeyCode::Backspace => {
                        ratatui::crossterm::event::KeyCode::Backspace
                    }
                    crossterm::event::KeyCode::Esc => ratatui::crossterm::event::KeyCode::Esc,
                    crossterm::event::KeyCode::Left => ratatui::crossterm::event::KeyCode::Left,
                    crossterm::event::KeyCode::Right => ratatui::crossterm::event::KeyCode::Right,
                    crossterm::event::KeyCode::Up => ratatui::crossterm::event::KeyCode::Up,
                    crossterm::event::KeyCode::Down => ratatui::crossterm::event::KeyCode::Down,
                    other => {
                        eprintln!("Unsupported key: {:?}", other);
                        continue;
                    }
                },
                modifiers: ratatui::crossterm::event::KeyModifiers::from_bits(
                    event.modifiers.bits(),
                )
                .unwrap_or(ratatui::crossterm::event::KeyModifiers::NONE),
                kind: match event.kind {
                    crossterm::event::KeyEventKind::Press => {
                        ratatui::crossterm::event::KeyEventKind::Press
                    }
                    crossterm::event::KeyEventKind::Release => {
                        ratatui::crossterm::event::KeyEventKind::Release
                    }
                    crossterm::event::KeyEventKind::Repeat => {
                        ratatui::crossterm::event::KeyEventKind::Repeat
                    }
                },
                state: ratatui::crossterm::event::KeyEventState::from_bits(event.state.bits())
                    .unwrap_or(ratatui::crossterm::event::KeyEventState::empty()),
            };
            app.handle_input(ratatui_event)?;
        }
    }

    Ok(())
}
