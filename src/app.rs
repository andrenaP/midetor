use crate::error::EditorError;
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use rusqlite::Connection;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use tui_textarea::{Input, Key, TextArea};

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Complete,
}

#[derive(PartialEq)]
pub enum View {
    Editor,
    Info,
}

#[derive(PartialEq, Clone, Debug)]
pub enum CompletionType {
    None,
    File,
    Tag,
}

pub struct App {
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
    pub should_quit: bool,
    history: Vec<(String, i64)>, // (file_path, file_id)
    history_index: usize,        // Current position in history
    completion_state: CompletionState,
}

pub struct CompletionState {
    active: bool,
    completion_type: CompletionType,
    query: String,
    suggestions: Vec<String>,
    list_state: ListState,
    trigger_start: (usize, usize), // (row, col) where trigger started
}

impl App {
    pub fn new(file_path: &str, base_dir: &str) -> Result<Self, EditorError> {
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
            completion_state: CompletionState {
                active: false,
                completion_type: CompletionType::None,
                query: String::new(),
                suggestions: Vec::new(),
                list_state: ListState::default(),
                trigger_start: (0, 0),
            },
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
        let mut textarea = TextArea::new(content.lines().map(|s| s.to_string()).collect());
        textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Markdown Editor")
                .style(Style::default().fg(Color::White)),
        );
        textarea.set_cursor_line_style(Style::default());
        textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        textarea.move_cursor(tui_textarea::CursorMove::Jump(0, 0)); // Reset cursor
                                                                    // Clear undo/redo history
        while textarea.undo() {}
        self.textarea = textarea; // Replace with fresh TextArea
        self.completion_state = CompletionState {
            active: false,
            completion_type: CompletionType::None,
            query: String::new(),
            suggestions: Vec::new(),
            list_state: ListState::default(),
            trigger_start: (0, 0),
        };
        self.tags = App::load_tags(&self.db, self.file_id)?;
        self.backlinks = App::load_backlinks(&self.db, self.file_id)?;
        self.view = View::Editor;
        // self.status = format!("Opened {}", self.file_path);
        self.mode = Mode::Normal;
        self.status = "Normal".to_string();
        Ok(())
    }

    fn follow_backlink(&mut self, index: usize) -> Result<(), EditorError> {
        if index < self.backlinks.len() {
            // Clean incomplete autocompletions
            let mut new_lines = self.textarea.lines().to_vec();
            let current_row = self.textarea.cursor().0;
            let line = new_lines[current_row].clone();
            if line.contains("[[") && !line.contains("]]") {
                new_lines[current_row] = line[..line.rfind("[[").unwrap_or(line.len())].to_string();
                self.textarea = TextArea::new(new_lines.clone());
                self.textarea.set_block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Markdown Editor")
                        .style(Style::default().fg(Color::White)),
                );
                self.textarea.set_cursor_line_style(Style::default());
                self.textarea
                    .set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
                self.textarea
                    .move_cursor(tui_textarea::CursorMove::Jump(current_row as u16, 0));
            }
            // self.save_file()?;

            let backlink_id = self.backlinks[index].1;
            let mut stmt = self.db.prepare("SELECT path FROM files WHERE id = ?")?;
            let path: String = stmt.query_row([backlink_id], |row| row.get(0))?;
            drop(stmt);

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

    fn start_completion(&mut self, completion_type: CompletionType) {
        self.completion_state.active = true;
        self.completion_state.completion_type = completion_type;
        self.completion_state.query = String::new();
        self.completion_state.suggestions = Vec::new();
        self.completion_state.list_state = ListState::default();
        self.mode = Mode::Complete;
        self.completion_state.trigger_start = self.textarea.cursor();
        self.status = format!("Completing {:?}", self.completion_state.completion_type);
    }

    fn update_completion(&mut self) -> Result<(), EditorError> {
        let (row, col) = self.textarea.cursor();
        let line = self.textarea.lines()[row].clone();
        let query = if self.completion_state.completion_type == CompletionType::File {
            line.get(..col)
                .and_then(|s| s.rfind("[["))
                .map(|start| line[start + 2..col].to_string())
                .unwrap_or_default()
        } else {
            line.get(..col)
                .and_then(|s| s.rfind("#"))
                .map(|start| line[start + 1..col].to_string())
                .unwrap_or_default()
        };

        self.completion_state.query = query.clone();
        self.completion_state.suggestions = if query.len() >= 2 {
            let sql = match self.completion_state.completion_type {
                CompletionType::File => format!(
                    "SELECT path FROM files WHERE path LIKE '%{}%' LIMIT 10",
                    query
                ),
                CompletionType::Tag => {
                    format!("SELECT tag FROM tags WHERE tag LIKE '%{}%' LIMIT 10", query)
                }
                CompletionType::None => return Ok(()),
            };
            let mut stmt = self.db.prepare(&sql)?;
            let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
            rows.collect::<Result<Vec<_>, _>>()?
        } else {
            Vec::new()
        };

        if !self.completion_state.suggestions.is_empty() {
            self.completion_state.list_state.select(Some(0));
        } else {
            self.completion_state.list_state.select(None);
        }

        Ok(())
    }

    fn select_completion(&mut self) -> Result<(), EditorError> {
        if let Some(selected) = self.completion_state.list_state.selected() {
            if let Some(suggestion) = self.completion_state.suggestions.get(selected) {
                let (current_row, current_col) = self.textarea.cursor();
                let current_line = self.textarea.lines()[current_row].clone();

                // Find the most recent trigger in the current line
                let trigger_pos = if self.completion_state.completion_type == CompletionType::File {
                    current_line[..current_col].rfind("[[")
                } else {
                    current_line[..current_col].rfind("#")
                };

                if let Some(start) = trigger_pos {
                    // Modify the current line to remove the trigger and query (e.g., "[[rad")
                    let mut new_lines = self.textarea.lines().to_vec();
                    let new_line =
                        format!("{}{}", &current_line[..start], &current_line[current_col..]);
                    new_lines[current_row] = new_line;
                    self.textarea = TextArea::new(new_lines);
                    self.textarea.set_block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Markdown Editor")
                            .style(Style::default().fg(Color::White)),
                    );
                    self.textarea.set_cursor_line_style(Style::default());
                    self.textarea
                        .set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
                    // Move cursor to the position after the prefix (e.g., after "12345")
                    self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                        current_row as u16,
                        start as u16,
                    ));
                } else {
                    // Fallback: Delete query length backward from current position
                    let delete_len = self.completion_state.query.len()
                        + if self.completion_state.completion_type == CompletionType::File {
                            2
                        } else {
                            1
                        };
                    let new_col = current_col.saturating_sub(delete_len);
                    self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                        current_row as u16,
                        new_col as u16,
                    ));
                    for _ in 0..delete_len {
                        self.textarea.delete_char();
                    }
                }

                // Insert the full suggestion
                let insert_text = match self.completion_state.completion_type {
                    CompletionType::File => format!("[[{}]]", suggestion),
                    CompletionType::Tag => format!("#{}", suggestion),
                    CompletionType::None => String::new(),
                };
                self.textarea.insert_str(&insert_text);
            }
        }
        self.cancel_completion();
        Ok(())
    }

    fn cancel_completion(&mut self) {
        self.completion_state.active = false;
        self.completion_state.completion_type = CompletionType::None;
        self.completion_state.query = String::new();
        self.completion_state.suggestions = Vec::new();
        self.completion_state.list_state = ListState::default();
        self.mode = Mode::Insert;
        self.status = "Insert".to_string();
    }

    pub fn handle_input(
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
                            let current_row = self.textarea.cursor().0;
                            let line = self.textarea.lines()[current_row].clone();
                            if let Some(index) = self
                                .backlinks
                                .iter()
                                .position(|(text, _)| line.contains(text))
                            {
                                // Clean incomplete autocompletions
                                let mut new_lines = self.textarea.lines().to_vec();
                                if line.contains("[[") && !line.contains("]]") {
                                    new_lines[current_row] =
                                        line[..line.rfind("[[").unwrap_or(line.len())].to_string();
                                }
                                self.textarea = TextArea::new(new_lines);
                                self.textarea.set_block(
                                    Block::default()
                                        .borders(Borders::ALL)
                                        .title("Markdown Editor")
                                        .style(Style::default().fg(Color::White)),
                                );
                                self.textarea.set_cursor_line_style(Style::default());
                                self.textarea.set_cursor_style(
                                    Style::default().bg(Color::White).fg(Color::Black),
                                );
                                self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                                    current_row as u16,
                                    0,
                                ));
                                // Reset completion state
                                self.cancel_completion();
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
                ratatui::crossterm::event::KeyCode::Char(_) => {
                    let input = Input::from(event);
                    self.textarea.input(input);
                    let (row, col) = self.textarea.cursor();
                    let line = self.textarea.lines()[row].clone();
                    if line.get(..col).map_or(false, |s| s.ends_with("[[")) {
                        self.start_completion(CompletionType::File);
                        self.update_completion()?;
                    } else if line.get(..col).map_or(false, |s| s.ends_with("#")) {
                        self.start_completion(CompletionType::Tag);
                        self.update_completion()?;
                    } else if self.completion_state.active {
                        self.update_completion()?;
                    }
                }
                ratatui::crossterm::event::KeyCode::Backspace => {
                    let input = Input::from(event);
                    self.textarea.input(input);
                    if self.completion_state.active {
                        let (row, col) = self.textarea.cursor();
                        let line = self.textarea.lines()[row].clone();
                        if self.completion_state.completion_type == CompletionType::File
                            && !line.get(..col).map_or(false, |s| s.contains("[["))
                        {
                            self.cancel_completion();
                        } else if self.completion_state.completion_type == CompletionType::Tag
                            && !line.get(..col).map_or(false, |s| s.contains("#"))
                        {
                            self.cancel_completion();
                        } else {
                            self.update_completion()?;
                        }
                    }
                }
                _ => {
                    let input = Input::from(event);
                    self.textarea.input(input);
                    if self.completion_state.active {
                        self.update_completion()?;
                    }
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
            Mode::Complete => match event.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    self.cancel_completion();
                }
                ratatui::crossterm::event::KeyCode::Enter => {
                    self.select_completion()?;
                }
                ratatui::crossterm::event::KeyCode::Up => {
                    let selected = self.completion_state.list_state.selected().unwrap_or(0);
                    if selected > 0 {
                        self.completion_state.list_state.select(Some(selected - 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    let selected = self.completion_state.list_state.selected().unwrap_or(0);
                    if selected < self.completion_state.suggestions.len() - 1 {
                        self.completion_state.list_state.select(Some(selected + 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Char(_) => {
                    let input = Input::from(event);
                    self.textarea.input(input);
                    self.update_completion()?;
                }
                ratatui::crossterm::event::KeyCode::Backspace => {
                    let input = Input::from(event);
                    self.textarea.input(input);
                    let (row, col) = self.textarea.cursor();
                    let line = self.textarea.lines()[row].clone();
                    if self.completion_state.completion_type == CompletionType::File
                        && !line.get(..col).map_or(false, |s| s.contains("[["))
                    {
                        self.cancel_completion();
                    } else if self.completion_state.completion_type == CompletionType::Tag
                        && !line.get(..col).map_or(false, |s| s.contains("#"))
                    {
                        self.cancel_completion();
                    } else {
                        self.update_completion()?;
                    }
                }
                _ => {}
            },
        }
        Ok(())
    }

    pub fn render(
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
                if self.completion_state.active && !self.completion_state.suggestions.is_empty() {
                    let items: Vec<ListItem> = self
                        .completion_state
                        .suggestions
                        .iter()
                        .map(|s| ListItem::new(s.clone()))
                        .collect();
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(match self.completion_state.completion_type {
                                    CompletionType::File => "Files",
                                    CompletionType::Tag => "Tags",
                                    CompletionType::None => "",
                                })
                                .style(Style::default().fg(Color::White)),
                        )
                        .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
                    let popup_area = Rect {
                        x: chunks[0].x + 2,
                        y: chunks[0].y + self.textarea.cursor().0 as u16 + 2,
                        width: 40,
                        height: (self.completion_state.suggestions.len().min(5) + 2) as u16,
                    };
                    f.render_stateful_widget(
                        list,
                        popup_area,
                        &mut self.completion_state.list_state,
                    );
                }
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
