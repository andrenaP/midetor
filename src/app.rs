use crate::error::EditorError;
use chrono::{Duration, Local};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use rusqlite::params;
use rusqlite::Connection;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use tui_textarea::{CursorMove, Input, Key, TextArea};

#[derive(PartialEq)]
pub enum Mode {
    Normal,
    Insert,
    Command,
    Complete,
    Search,
    TagFiles,
    Visual,
    VisualBlock,
    BlockInsert,
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

#[derive(PartialEq, Clone, Debug)]
pub enum SearchType {
    None,
    Backlinks,
    Tags,
    Files,
}

#[derive(PartialEq, Clone, Debug)]
pub enum InsertPosition {
    Before,
    After,
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
    search_state: SearchState,
    key_sequence: String, // Tracks key sequence in Normal mode (e.g., "\", "\o", "\ob")
    tag_files: Vec<(String, i64)>, // Files associated with selected tag
    tag_files_state: ListState, // State for selecting tag files
    yanked: Vec<String>,
    visual_anchor: Option<(usize, usize)>,
    insert_position: InsertPosition,
    block_insert_col: usize,
    // Syntax highlighting fields
    syntax_set: SyntaxSet,
    theme: Theme, // Use Box to ensure 'static lifetime
    scroll_offset: usize,
}

pub struct CompletionState {
    active: bool,
    completion_type: CompletionType,
    query: String,
    suggestions: Vec<String>,
    list_state: ListState,
    trigger_start: (usize, usize), // (row, col) where trigger started
}

pub struct SearchState {
    active: bool,
    search_type: SearchType,
    query: String,
    results: Vec<(String, Option<i64>)>, // (display_text, file_id or None for tags)
    list_state: ListState,
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
        textarea.set_selection_style(Style::default().bg(Color::LightBlue));

        let file_id = App::get_file_id(&db, file_path)?;
        let tags = App::load_tags(&db, file_id)?;
        let backlinks = App::load_backlinks(&db, file_id)?;

        // Initialize syntax highlighting
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults(); // Load ThemeSet
        let theme = theme_set.themes["base16-eighties.dark"].clone();
        // let highlighter = HighlightLines::new(syntax, &theme); // Use reference to boxed Theme

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
            search_state: SearchState {
                active: false,
                search_type: SearchType::None,
                query: String::new(),
                results: Vec::new(),
                list_state: ListState::default(),
            },
            key_sequence: String::new(),
            tag_files: Vec::new(),
            tag_files_state: ListState::default(),
            yanked: Vec::new(),
            visual_anchor: None,
            insert_position: InsertPosition::Before,
            block_insert_col: 0,
            syntax_set,
            theme,
            scroll_offset: 0, // Initialize scroll offset
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
        textarea.set_selection_style(Style::default().bg(Color::LightBlue));
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
        self.mode = Mode::Normal;
        self.status = "Normal".to_string();
        Ok(())
    }

    fn open_wikilink_file(&mut self, wikilink: String) -> Result<(), EditorError> {
        // Normalize the wikilink to a file path
        let path = if wikilink.ends_with(".md") {
            wikilink.clone() // Preserve path as-is if it includes .md
        } else {
            format!("{}.md", wikilink) // Append .md if not present
        };

        // Try the wikilink as a relative path (e.g., Every day info/2023-04-18.md)
        let db_path = path.clone();
        let full_path = Path::new(&self.base_dir)
            .join(&path)
            .to_string_lossy()
            .to_string();

        // Check if the file exists in the database with the exact path
        let file_id_result = {
            let mut stmt = self.db.prepare("SELECT id FROM files WHERE path = ?")?;
            stmt.query_row([&db_path], |row| row.get(0))
        };

        let file_id = match file_id_result {
            Ok(id) => id,
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // Try finding the file by basename (e.g., 2023-04-18.md anywhere in the vault)
                let basename = Path::new(&path)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let mut stmt = self
                    .db
                    .prepare("SELECT id, path FROM files WHERE path LIKE ?")?;
                let file_result = stmt.query_row([format!("%/{}", basename)], |row| {
                    Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
                });
                drop(stmt);

                match file_result {
                    Ok((id, found_path)) => {
                        // Update full_path to the actual file location
                        let full_path = Path::new(&self.base_dir)
                            .join(&found_path)
                            .to_string_lossy()
                            .to_string();
                        self.history.truncate(self.history_index + 1);
                        self.history.push((full_path.clone(), id));
                        self.history_index += 1;
                        self.open_file(full_path, id)?;
                        return Ok(());
                    }
                    Err(rusqlite::Error::QueryReturnedNoRows) => {
                        // File doesn't exist; create it
                        if let Some(parent) = Path::new(&full_path).parent() {
                            fs::create_dir_all(parent)?;
                        }
                        fs::write(&full_path, "")?;
                        let output = Command::new("markdown-scanner")
                            .arg(&full_path)
                            .arg(&self.base_dir)
                            .output()?;
                        if !output.status.success() {
                            let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
                            return Err(EditorError::Scanner(error_msg));
                        }
                        let mut stmt = self.db.prepare("SELECT id FROM files WHERE path = ?")?;
                        stmt.query_row([&db_path], |row| row.get(0))
                            .map_err(|e| EditorError::Database(e))?
                    }
                    Err(e) => return Err(EditorError::Database(e)),
                }
            }
            Err(e) => return Err(EditorError::Database(e)),
        };

        self.history.truncate(self.history_index + 1);
        self.history.push((full_path.clone(), file_id));
        self.history_index += 1;
        self.open_file(full_path, file_id)?;
        Ok(())
    }

    fn follow_backlink(&mut self, index: usize) -> Result<(), EditorError> {
        let current_row = self.textarea.cursor().0;
        let line = self.textarea.lines()[current_row].clone();

        // Extract wikilink from the current line if no valid backlink index
        let wikilink = if index >= self.backlinks.len() {
            self.extract_wikilink(&line).ok_or_else(|| {
                EditorError::InvalidBacklink("No valid wikilink found".to_string())
            })?
        } else {
            self.backlinks[index].0.clone()
        };

        // Clean incomplete autocompletions
        if line.contains("[[") && !line.contains("]]") {
            let mut new_lines = self.textarea.lines().to_vec();
            new_lines[current_row] = line[..line.rfind("[[").unwrap_or(line.len())].to_string();
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
            self.textarea
                .move_cursor(tui_textarea::CursorMove::Jump(current_row as u16, 0));
        }

        self.open_wikilink_file(wikilink)?;

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

        // Convert character index to byte index
        let col_bytes = line
            .char_indices()
            .nth(col)
            .map(|(i, _)| i)
            .unwrap_or(line.len());

        let query = if self.completion_state.completion_type == CompletionType::File {
            line.get(..col_bytes)
                .and_then(|s| s.rfind("[["))
                .map(|start| line[start + 2..col_bytes].to_string())
                .unwrap_or_default()
        } else {
            line.get(..col_bytes)
                .and_then(|s| s.rfind("#"))
                .map(|start| line[start + 1..col_bytes].to_string())
                .unwrap_or_default()
        };

        self.completion_state.query = query.clone();
        self.completion_state.suggestions = if query.len() >= 2 {
            let search_pattern = format!("%{}%", query);
            let sql = match self.completion_state.completion_type {
                CompletionType::File => {
                    "SELECT DISTINCT result FROM (
                        SELECT file_name AS result FROM files WHERE file_name LIKE ?
                        UNION
                        SELECT backlink AS result FROM backlinks WHERE backlink LIKE ?
                    ) LIMIT 10"
                }
                CompletionType::Tag => "SELECT tag FROM tags WHERE tag LIKE ? LIMIT 10",
                CompletionType::None => return Ok(()),
            };
            let mut stmt = self.db.prepare(sql)?;
            let closure = |row: &rusqlite::Row| row.get::<_, String>(0);
            let rows = if self.completion_state.completion_type == CompletionType::File {
                stmt.query_map(params![search_pattern, search_pattern], closure)?
            } else {
                stmt.query_map(params![search_pattern], closure)?
            };
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

                // Convert character index to byte index
                let col_bytes = current_line
                    .char_indices()
                    .nth(current_col)
                    .map(|(i, _)| i)
                    .unwrap_or(current_line.len());

                // Find the most recent trigger in the current line up to cursor
                let trigger_pos = if self.completion_state.completion_type == CompletionType::File {
                    current_line[..col_bytes].rfind("[[")
                } else {
                    current_line[..col_bytes].rfind("#")
                };

                if let Some(start) = trigger_pos {
                    // Remove text from trigger to current cursor position (in bytes)
                    let mut new_lines = self.textarea.lines().to_vec();
                    let new_line =
                        format!("{}{}", &current_line[..start], &current_line[col_bytes..]);
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
                    self.textarea
                        .set_selection_style(Style::default().bg(Color::LightBlue));

                    // Calculate new cursor position in characters (after the trigger)
                    let prefix_chars = current_line[..start].chars().count();
                    self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                        current_row as u16,
                        prefix_chars as u16,
                    ));

                    // Insert the full suggestion
                    let insert_text = match self.completion_state.completion_type {
                        CompletionType::File => format!("[[{}]]", suggestion),
                        CompletionType::Tag => format!("#{}", suggestion),
                        CompletionType::None => String::new(),
                    };
                    self.textarea.insert_str(&insert_text);
                } else {
                    // Fallback: Delete the query and trigger
                    let delete_len = self.completion_state.query.len()
                        + if self.completion_state.completion_type == CompletionType::File {
                            2 // Length of "[[" trigger
                        } else {
                            1 // Length of "#" trigger
                        };
                    let new_col = current_col.saturating_sub(delete_len);
                    self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                        current_row as u16,
                        new_col as u16,
                    ));
                    for _ in 0..delete_len {
                        self.textarea.delete_char();
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

    fn load_tag_files(&mut self, tag: &str) -> Result<(), EditorError> {
        let mut stmt = self.db.prepare(
            "SELECT f.path, f.id FROM files f
             JOIN file_tags ft ON f.id = ft.file_id
             JOIN tags t ON ft.tag_id = t.id
             WHERE t.tag = ?",
        )?;
        let files = stmt
            .query_map([tag], |row| {
                let path: String = row.get(0)?;
                let file_id: i64 = row.get(1)?;
                Ok((path, file_id))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        self.tag_files = files;
        self.tag_files_state = ListState::default();
        if !self.tag_files.is_empty() {
            self.tag_files_state.select(Some(0));
            self.status = format!("Select file for tag '{}'", tag);
        } else {
            self.status = format!("No files found for tag '{}'", tag);
        }
        Ok(())
    }

    fn select_tag_file(&mut self) -> Result<(), EditorError> {
        if let Some(selected) = self.tag_files_state.selected() {
            if let Some((path, file_id)) = self.tag_files.get(selected) {
                self.history.truncate(self.history_index + 1);
                self.history.push((path.clone(), *file_id));
                self.history_index += 1;
                self.open_file(path.clone(), *file_id)?;
            }
        }
        self.cancel_tag_files();
        Ok(())
    }

    fn cancel_tag_files(&mut self) {
        self.tag_files.clear();
        self.tag_files_state = ListState::default();
        self.mode = Mode::Normal;
        self.status = "Normal".to_string();
        self.key_sequence.clear();
    }

    fn start_search(&mut self, search_type: SearchType) -> Result<(), EditorError> {
        self.search_state.active = true;
        self.search_state.search_type = search_type.clone();
        self.search_state.query = String::new();
        self.search_state.results = Vec::new();
        self.search_state.list_state = ListState::default();
        self.mode = Mode::Search;
        self.status = format!("Searching {:?}", search_type);
        self.key_sequence.clear();
        self.update_search_results()?;
        Ok(())
    }

    fn update_search_results(&mut self) -> Result<(), EditorError> {
        match self.search_state.search_type {
            SearchType::Backlinks => self.search_backlinks()?,
            SearchType::Tags => self.search_tags()?,
            SearchType::Files => self.search_files()?,
            SearchType::None => {}
        }
        if !self.search_state.results.is_empty() {
            self.search_state.list_state.select(Some(0));
        } else {
            self.search_state.list_state.select(None);
        }
        Ok(())
    }

    fn search_backlinks(&mut self) -> Result<(), EditorError> {
        let (row, _col) = self.textarea.cursor();
        let line = self.textarea.lines()[row].clone();
        let target = if let Some(wikilink) = self.extract_wikilink(&line) {
            wikilink
        } else {
            self.file_path.clone()
        };

        let query = "SELECT DISTINCT f.path, f.id
                     FROM backlinks b
                     JOIN files f ON b.file_id = f.id
                     JOIN files fp ON b.backlink_id = fp.id
                     WHERE fp.path LIKE ?";
        let mut stmt = self.db.prepare(query)?;
        let results = stmt
            .query_map([format!("%{}%", target)], |row| {
                let path: String = row.get(0)?;
                let file_id: i64 = row.get(1)?;
                Ok((path, Some(file_id)))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        self.search_state.results = results;
        Ok(())
    }

    fn search_tags(&mut self) -> Result<(), EditorError> {
        let query = "SELECT DISTINCT tag FROM tags WHERE tag LIKE ? LIMIT 10";
        let mut stmt = self.db.prepare(query)?;
        let search_pattern = if self.search_state.query.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", self.search_state.query)
        };
        let results = stmt
            .query_map(params![search_pattern], |row| {
                let tag: String = row.get(0)?;
                Ok((tag, None))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        self.search_state.results = results;
        Ok(())
    }

    fn search_files(&mut self) -> Result<(), EditorError> {
        let query = "SELECT path, id FROM files WHERE path LIKE ? LIMIT 10";
        let mut stmt = self.db.prepare(query)?;
        let search_pattern = if self.search_state.query.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", self.search_state.query)
        };
        let results = stmt
            .query_map(params![search_pattern], |row| {
                let path: String = row.get(0)?;
                let file_id: i64 = row.get(1)?;
                Ok((path, Some(file_id)))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        self.search_state.results = results;
        Ok(())
    }

    fn extract_wikilink(&self, line: &str) -> Option<String> {
        let start = line.find("[[")?;
        let end = line[start..].find("]]").map(|i| i + start + 2)?;
        Some(line[start + 2..end - 2].to_string())
    }

    fn select_search_result(&mut self) -> Result<(), EditorError> {
        if let Some(selected) = self.search_state.list_state.selected() {
            if let Some((result, file_id)) = self.search_state.results.get(selected).cloned() {
                match self.search_state.search_type {
                    SearchType::Backlinks | SearchType::Files => {
                        if let Some(file_id) = file_id {
                            self.history.truncate(self.history_index + 1);
                            self.history.push((result.clone(), file_id));
                            self.history_index += 1;
                            self.open_file(result, file_id)?;
                        }
                    }
                    SearchType::Tags => {
                        self.load_tag_files(&result)?;
                        if !self.tag_files.is_empty() {
                            self.mode = Mode::TagFiles;
                        } else {
                            self.cancel_search();
                        }
                    }
                    SearchType::None => {}
                }
            }
        } else {
            self.status = "No tag selected".to_string();
        }
        if self.mode != Mode::TagFiles {
            self.cancel_search();
        }
        Ok(())
    }

    fn cancel_search(&mut self) {
        self.search_state.active = false;
        self.search_state.search_type = SearchType::None;
        self.search_state.query = String::new();
        self.search_state.results = Vec::new();
        self.search_state.list_state = ListState::default();
        self.mode = Mode::Normal;
        self.status = "Normal".to_string();
        self.key_sequence.clear();
    }

    pub fn handle_input(
        &mut self,
        event: ratatui::crossterm::event::KeyEvent,
    ) -> Result<(), EditorError> {
        match self.mode {
            Mode::Normal => {
                // Handle key sequence first if it has started
                if !self.key_sequence.is_empty() {
                    if let ratatui::crossterm::event::KeyCode::Char(c) = event.code {
                        self.key_sequence.push(c);
                        let sequence = self.key_sequence.clone(); // Clone to avoid borrow issues
                        match sequence.as_str() {
                            "gg" => {
                                self.textarea.move_cursor(CursorMove::Top);
                                self.key_sequence.clear();
                                self.status = "Moved to top".to_string();
                            }
                            "yy" => {
                                let row = self.textarea.cursor().0;
                                self.yanked = vec![self.textarea.lines()[row].clone()];
                                self.key_sequence.clear();
                                self.status = "Yanked line (not undoable)".to_string();
                            }
                            "dd" => {
                                self.textarea.move_cursor(CursorMove::Head);
                                self.textarea.delete_line_by_end();
                                self.key_sequence.clear();
                                self.status = "Deleted line".to_string();
                            }
                            "\\ob" => {
                                self.start_search(SearchType::Backlinks)?;
                                self.key_sequence.clear();
                                self.status = "Started backlinks search".to_string();
                            }
                            "\\ot" => {
                                self.start_search(SearchType::Tags)?;
                                self.key_sequence.clear();
                                self.status = "Started tags search".to_string();
                            }
                            "\\f" => {
                                self.start_search(SearchType::Files)?;
                                self.key_sequence.clear();
                                self.status = "Started files search".to_string();
                            }
                            "\\oot" => {
                                let today = Local::now()
                                    .format("Every day info/%Y-%m-%d.md")
                                    .to_string();
                                self.open_wikilink_file(today)?;
                                self.key_sequence.clear();
                                self.status = "Opened today's file".to_string();
                            }
                            "\\ooy" => {
                                let yesterday = (Local::now() - Duration::days(1))
                                    .format("Every day info/%Y-%m-%d.md")
                                    .to_string();
                                self.open_wikilink_file(yesterday)?;
                                self.key_sequence.clear();
                                self.status = "Opened yesterday's file".to_string();
                            }
                            "\\ooT" => {
                                let tomorrow = (Local::now() + Duration::days(1))
                                    .format("Every day info/%Y-%m-%d.md")
                                    .to_string();
                                self.open_wikilink_file(tomorrow)?;
                                self.key_sequence.clear();
                                self.status = "Opened tomorrow's file".to_string();
                            }
                            s if !("\\ob".starts_with(s)
                                || "\\ot".starts_with(s)
                                || "\\f".starts_with(s)
                                || "\\oot".starts_with(s)
                                || "\\ooy".starts_with(s)
                                || "\\ooT".starts_with(s)) =>
                            {
                                self.key_sequence.clear();
                                self.status = format!("Invalid sequence 1: {}", s);
                            }
                            _ => {}
                        }
                        return Ok(());
                    }
                }

                // Handle single-key commands and other key events
                match (event.code, event.modifiers) {
                    (
                        ratatui::crossterm::event::KeyCode::Char('o'),
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.navigate_back()?;
                    }
                    (
                        ratatui::crossterm::event::KeyCode::Char('i'),
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.navigate_forward()?;
                    }
                    (
                        ratatui::crossterm::event::KeyCode::Char('r'),
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        if self.textarea.redo() {
                            self.status = "Redone".to_string();
                        } else {
                            self.status = "Nothing to redo".to_string();
                        }
                    }
                    (ratatui::crossterm::event::KeyCode::Char('u'), _) => {
                        if self.textarea.undo() {
                            self.status = "Undone".to_string();
                        } else {
                            self.status = "Nothing to undo".to_string();
                        }
                    }
                    (ratatui::crossterm::event::KeyCode::Char('i'), _) => {
                        self.mode = Mode::Insert;
                        self.status = "Insert".to_string();
                    }
                    (ratatui::crossterm::event::KeyCode::Char('a'), _) => {
                        self.textarea.move_cursor(CursorMove::Forward);
                        self.mode = Mode::Insert;
                        self.status = "Insert".to_string();
                    }
                    (ratatui::crossterm::event::KeyCode::Char('o'), _) => {
                        self.textarea.move_cursor(CursorMove::End);
                        self.textarea.insert_newline();
                        self.mode = Mode::Insert;
                        self.status = "Insert".to_string();
                    }
                    (
                        ratatui::crossterm::event::KeyCode::Char('v'),
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.visual_anchor = Some(self.textarea.cursor());
                        self.mode = Mode::VisualBlock;
                        self.status = "Visual Block".to_string();
                    }
                    (ratatui::crossterm::event::KeyCode::Char('v'), _) => {
                        self.visual_anchor = Some(self.textarea.cursor());
                        self.mode = Mode::Visual;
                        self.status = "Visual".to_string();
                    }
                    (ratatui::crossterm::event::KeyCode::Char('x'), _) => {
                        self.textarea.delete_next_char();
                    }
                    (ratatui::crossterm::event::KeyCode::Char('p'), _) => {
                        self.textarea.move_cursor(CursorMove::End);
                        self.textarea.insert_char('\n');
                        self.textarea.insert_str(&self.yanked.join("\n"));
                    }
                    (ratatui::crossterm::event::KeyCode::Char(':'), _) => {
                        self.mode = Mode::Command;
                        self.command.clear();
                        self.status = "Command".to_string();
                    }
                    (ratatui::crossterm::event::KeyCode::Char('j'), _) => {
                        self.textarea.move_cursor(CursorMove::Down);
                    }
                    (ratatui::crossterm::event::KeyCode::Char('k'), _) => {
                        self.textarea.move_cursor(CursorMove::Up);
                    }
                    (ratatui::crossterm::event::KeyCode::Char('h'), _) => {
                        self.textarea.move_cursor(CursorMove::Back);
                    }
                    (ratatui::crossterm::event::KeyCode::Char('l'), _) => {
                        self.textarea.move_cursor(CursorMove::Forward);
                    }
                    (ratatui::crossterm::event::KeyCode::Up, _) => {
                        self.textarea.move_cursor(CursorMove::Up);
                        // self.scroll_offset = self.scroll_offset.saturating_sub(1);
                    }
                    (ratatui::crossterm::event::KeyCode::Down, _) => {
                        self.textarea.move_cursor(CursorMove::Down);
                    }
                    (
                        ratatui::crossterm::event::KeyCode::Left,
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.textarea.move_cursor(CursorMove::WordBack);
                    }
                    (
                        ratatui::crossterm::event::KeyCode::Right,
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.textarea.move_cursor(CursorMove::WordForward);
                    }
                    (ratatui::crossterm::event::KeyCode::Left, _) => {
                        self.textarea.move_cursor(CursorMove::Back);
                    }
                    (ratatui::crossterm::event::KeyCode::Right, _) => {
                        self.textarea.move_cursor(CursorMove::Forward);
                    }
                    (
                        ratatui::crossterm::event::KeyCode::Home,
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.textarea.move_cursor(CursorMove::Top);
                    }
                    (ratatui::crossterm::event::KeyCode::Home, _) => {
                        self.textarea.move_cursor(CursorMove::Head);
                    }
                    (
                        ratatui::crossterm::event::KeyCode::End,
                        ratatui::crossterm::event::KeyModifiers::CONTROL,
                    ) => {
                        self.textarea.move_cursor(CursorMove::Bottom);
                    }
                    (ratatui::crossterm::event::KeyCode::End, _) => {
                        self.textarea.move_cursor(CursorMove::End);
                    }
                    (ratatui::crossterm::event::KeyCode::Char('G'), _)
                        if self.key_sequence.is_empty() =>
                    {
                        self.textarea.move_cursor(CursorMove::Bottom);
                    }
                    (ratatui::crossterm::event::KeyCode::Char(c), _) => {
                        //TODO IF I DELETE THAT THAN gg dd and yy in not working.
                        self.key_sequence.push(c);
                        let sequence = self.key_sequence.clone(); // Clone to avoid borrow issues
                        match sequence.as_str() {
                            _ => {}
                        }
                    }
                    (ratatui::crossterm::event::KeyCode::Enter, _) => {
                        if self.view == View::Editor {
                            let current_row = self.textarea.cursor().0;
                            let line = self.textarea.lines()[current_row].clone();
                            if let Some(index) = self
                                .backlinks
                                .iter()
                                .position(|(text, _)| line.contains(text))
                            {
                                self.follow_backlink(index)?;
                            } else if let Some(_wikilink) = self.extract_wikilink(&line) {
                                // Handle wikilink not in backlinks by passing an invalid index
                                self.follow_backlink(usize::MAX)?;
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
                    _ => {
                        self.status = format!("Unsupported key: {:?}", event.code);
                    }
                }
            }
            Mode::Insert => {
                let input = Input::from(event);
                match event.code {
                    ratatui::crossterm::event::KeyCode::Esc => {
                        self.mode = Mode::Normal;
                        self.status = "Normal".to_string();
                    }
                    ratatui::crossterm::event::KeyCode::Char(_) => {
                        self.textarea.input(input);
                        let (row, col) = self.textarea.cursor();
                        let line = self.textarea.lines()[row].clone();
                        // Convert character index to byte index
                        let col_bytes = line
                            .char_indices()
                            .nth(col)
                            .map(|(i, _)| i)
                            .unwrap_or(line.len());
                        if line.get(..col_bytes).map_or(false, |s| s.ends_with("[[")) {
                            self.start_completion(CompletionType::File);
                            self.update_completion()?;
                        } else if line.get(..col_bytes).map_or(false, |s| s.ends_with("#")) {
                            self.start_completion(CompletionType::Tag);
                            self.update_completion()?;
                        } else if self.completion_state.active {
                            self.update_completion()?;
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Backspace => {
                        self.textarea.input(input);
                        if self.completion_state.active {
                            let (row, col) = self.textarea.cursor();
                            let line = self.textarea.lines()[row].clone();
                            // Convert character index to byte index
                            let col_bytes = line
                                .char_indices()
                                .nth(col)
                                .map(|(i, _)| i)
                                .unwrap_or(line.len());
                            if self.completion_state.completion_type == CompletionType::File
                                && !line.get(..col_bytes).map_or(false, |s| s.contains("[["))
                            {
                                self.cancel_completion();
                            } else if self.completion_state.completion_type == CompletionType::Tag
                                && !line.get(..col_bytes).map_or(false, |s| s.contains("#"))
                            {
                                self.cancel_completion();
                            } else {
                                self.update_completion()?;
                            }
                        }
                    }
                    _ => {
                        self.textarea.input(input);
                        if self.completion_state.active {
                            self.update_completion()?;
                        }
                    }
                }
            }
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
                    // Convert character index to byte index
                    let col_bytes = line
                        .char_indices()
                        .nth(col)
                        .map(|(i, _)| i)
                        .unwrap_or(line.len());
                    if self.completion_state.completion_type == CompletionType::File
                        && !line.get(..col_bytes).map_or(false, |s| s.contains("[["))
                    {
                        self.cancel_completion();
                    } else if self.completion_state.completion_type == CompletionType::Tag
                        && !line.get(..col_bytes).map_or(false, |s| s.contains("#"))
                    {
                        self.cancel_completion();
                    } else {
                        self.update_completion()?;
                    }
                }
                _ => {}
            },
            Mode::Search => match event.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    self.cancel_search();
                }
                ratatui::crossterm::event::KeyCode::Enter => {
                    self.select_search_result()?;
                }
                ratatui::crossterm::event::KeyCode::Up => {
                    let selected = self.search_state.list_state.selected().unwrap_or(0);
                    if selected > 0 {
                        self.search_state.list_state.select(Some(selected - 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    let selected = self.search_state.list_state.selected().unwrap_or(0);
                    if selected < self.search_state.results.len() - 1 {
                        self.search_state.list_state.select(Some(selected + 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Char(c) => {
                    self.search_state.query.push(c);
                    self.update_search_results()?;
                }
                ratatui::crossterm::event::KeyCode::Backspace => {
                    self.search_state.query.pop();
                    self.update_search_results()?;
                }
                _ => {}
            },
            Mode::TagFiles => match event.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    self.cancel_tag_files();
                }
                ratatui::crossterm::event::KeyCode::Enter => {
                    self.select_tag_file()?;
                }
                ratatui::crossterm::event::KeyCode::Up => {
                    let selected = self.tag_files_state.selected().unwrap_or(0);
                    if selected > 0 {
                        self.tag_files_state.select(Some(selected - 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    let selected = self.tag_files_state.selected().unwrap_or(0);
                    if selected < self.tag_files.len() - 1 {
                        self.tag_files_state.select(Some(selected + 1));
                    }
                }
                _ => {}
            },
            Mode::Visual | Mode::VisualBlock => {
                let mut input = Input::from(event);
                match input.key {
                    Key::Esc => {
                        self.textarea.cancel_selection();
                        self.visual_anchor = None;
                        self.mode = Mode::Normal;
                        self.status = "Normal".to_string();
                    }
                    Key::Char('y') => {
                        let (min_row, min_col, max_row, max_col) =
                            if let Some(anchor) = self.visual_anchor {
                                let cursor = self.textarea.cursor();
                                (
                                    anchor.0.min(cursor.0),
                                    anchor.1.min(cursor.1),
                                    anchor.0.max(cursor.0),
                                    anchor.1.max(cursor.1),
                                )
                            } else {
                                return Ok(());
                            };
                        if self.mode == Mode::VisualBlock {
                            self.yanked = (min_row..=max_row)
                                .map(|row| {
                                    let line = &self.textarea.lines()[row];
                                    let start_byte = line
                                        .char_indices()
                                        .nth(min_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(line.len());
                                    let end_byte = line
                                        .char_indices()
                                        .nth(max_col + 1)
                                        .map(|(b, _)| b)
                                        .unwrap_or(line.len());
                                    line[start_byte..end_byte].to_string()
                                })
                                .collect();
                        } else {
                            if let Some(range) = self.textarea.selection_range() {
                                let start_row = range.0 .0;
                                let start_col = range.0 .1;
                                let end_row = range.1 .0;
                                let end_col = range.1 .1;
                                if start_row == end_row {
                                    let line = self.textarea.lines()[start_row].clone();
                                    let start_byte = line
                                        .char_indices()
                                        .nth(start_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(0);
                                    let end_byte = line
                                        .char_indices()
                                        .nth(end_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(line.len());
                                    self.yanked = vec![line[start_byte..end_byte].to_string()];
                                } else {
                                    let mut yanked = Vec::new();
                                    for row in start_row..=end_row {
                                        let line = self.textarea.lines()[row].clone();
                                        if row == start_row {
                                            let start_byte = line
                                                .char_indices()
                                                .nth(start_col)
                                                .map(|(b, _)| b)
                                                .unwrap_or(0);
                                            yanked.push(line[start_byte..].to_string());
                                        } else if row == end_row {
                                            let end_byte = line
                                                .char_indices()
                                                .nth(end_col)
                                                .map(|(b, _)| b)
                                                .unwrap_or(line.len());
                                            yanked.push(line[..end_byte].to_string());
                                        } else {
                                            yanked.push(line);
                                        }
                                    }
                                    self.yanked = yanked;
                                }
                            }
                        }
                        self.textarea.cancel_selection();
                        self.visual_anchor = None;
                        self.mode = Mode::Normal;
                        self.status = "Yanked (not undoable)".to_string();
                    }
                    Key::Char('x') | Key::Char('d') => {
                        let (min_row, min_col, max_row, max_col) =
                            if let Some(anchor) = self.visual_anchor {
                                let cursor = self.textarea.cursor();
                                (
                                    anchor.0.min(cursor.0),
                                    anchor.1.min(cursor.1),
                                    anchor.0.max(cursor.0),
                                    anchor.1.max(cursor.1),
                                )
                            } else {
                                return Ok(());
                            };
                        if self.mode == Mode::VisualBlock {
                            for row in (min_row..=max_row).rev() {
                                let line = self.textarea.lines()[row].clone();
                                let start_byte = line
                                    .char_indices()
                                    .nth(min_col)
                                    .map(|(b, _)| b)
                                    .unwrap_or(line.len());
                                let end_byte = line
                                    .char_indices()
                                    .nth(max_col + 1)
                                    .map(|(b, _)| b)
                                    .unwrap_or(line.len());
                                let new_line =
                                    format!("{}{}", &line[0..start_byte], &line[end_byte..]);
                                self.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
                                self.textarea.delete_line_by_end();
                                self.textarea.insert_str(&new_line);
                            }
                            self.textarea
                                .move_cursor(CursorMove::Jump(min_row as u16, min_col as u16));
                        } else {
                            // Custom delete selection logic
                            if let Some(range) = self.textarea.selection_range() {
                                let start_row = range.0 .0;
                                let start_col = range.0 .1;
                                let end_row = range.1 .0;
                                let end_col = range.1 .1;
                                let mut new_lines = self.textarea.lines().to_vec();
                                if start_row == end_row {
                                    let line = new_lines[start_row].clone();
                                    let start_byte = line
                                        .char_indices()
                                        .nth(start_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(0);
                                    let end_byte = line
                                        .char_indices()
                                        .nth(end_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(line.len());
                                    new_lines[start_row] =
                                        format!("{}{}", &line[..start_byte], &line[end_byte..]);
                                } else {
                                    // Delete from start_row to end_row
                                    let first_line = new_lines[start_row].clone();
                                    let last_line = new_lines[end_row].clone();
                                    let start_byte = first_line
                                        .char_indices()
                                        .nth(start_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(0);
                                    let end_byte = last_line
                                        .char_indices()
                                        .nth(end_col)
                                        .map(|(b, _)| b)
                                        .unwrap_or(last_line.len());
                                    new_lines[start_row] = format!(
                                        "{}{}",
                                        &first_line[..start_byte],
                                        &last_line[end_byte..]
                                    );
                                    new_lines.drain((start_row + 1)..=end_row);
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
                                self.textarea
                                    .set_selection_style(Style::default().bg(Color::LightBlue));
                                self.textarea.move_cursor(CursorMove::Jump(
                                    start_row as u16,
                                    start_col as u16,
                                ));
                            }
                        }
                        self.visual_anchor = None;
                        self.mode = Mode::Normal;
                        self.status = "Deleted".to_string();
                    }
                    Key::Char('I') if self.mode == Mode::VisualBlock => {
                        self.insert_position = InsertPosition::Before;
                        let (min_row, min_col, _, _) = if let Some(anchor) = self.visual_anchor {
                            let cursor = self.textarea.cursor();
                            (
                                anchor.0.min(cursor.0),
                                anchor.1.min(cursor.1),
                                anchor.0.max(cursor.0),
                                anchor.1.max(cursor.1),
                            )
                        } else {
                            return Ok(());
                        };
                        self.block_insert_col = min_col;
                        self.textarea.cancel_selection();
                        self.textarea
                            .move_cursor(CursorMove::Jump(min_row as u16, min_col as u16));
                        self.mode = Mode::BlockInsert;
                        self.status = "Block Insert Before".to_string();
                    }
                    Key::Char('A') if self.mode == Mode::VisualBlock => {
                        self.insert_position = InsertPosition::After;
                        let (min_row, _, _, max_col) = if let Some(anchor) = self.visual_anchor {
                            let cursor = self.textarea.cursor();
                            (
                                anchor.0.min(cursor.0),
                                anchor.1.min(cursor.1),
                                anchor.0.max(cursor.0),
                                anchor.1.max(cursor.1),
                            )
                        } else {
                            return Ok(());
                        };
                        self.block_insert_col = max_col + 1;
                        self.textarea.cancel_selection();
                        self.textarea.move_cursor(CursorMove::Jump(
                            min_row as u16,
                            self.block_insert_col as u16,
                        ));
                        self.mode = Mode::BlockInsert;
                        self.status = "Block Insert After".to_string();
                    }
                    _ => {
                        input.shift = true;
                        self.textarea.input(input);
                    }
                }
            }
            Mode::BlockInsert => {
                let input = Input::from(event);
                let (min_row, min_col, max_row, max_col) = if let Some(anchor) = self.visual_anchor
                {
                    let cursor = self.textarea.cursor();
                    (
                        anchor.0.min(cursor.0),
                        anchor.1.min(cursor.1),
                        anchor.0.max(cursor.0),
                        anchor.1.max(cursor.1),
                    )
                } else {
                    return Ok(());
                };
                let original_col = match self.insert_position {
                    InsertPosition::Before => min_col,
                    InsertPosition::After => max_col + 1,
                };
                match event.code {
                    ratatui::crossterm::event::KeyCode::Esc => {
                        self.visual_anchor = None;
                        self.mode = Mode::Normal;
                        self.status = "Normal".to_string();
                    }
                    ratatui::crossterm::event::KeyCode::Char(c) => {
                        for row in min_row..=max_row {
                            let line = self.textarea.lines()[row].clone();
                            let target_col = self.block_insert_col;
                            let start_byte = line
                                .char_indices()
                                .nth(target_col)
                                .map(|(b, _)| b)
                                .unwrap_or(line.len());
                            let mut new_line = line.clone();
                            if target_col > line.len() {
                                new_line.push_str(&" ".repeat(target_col - line.len()));
                            }
                            new_line.insert_str(start_byte, &c.to_string());
                            self.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
                            self.textarea.delete_line_by_end();
                            self.textarea.insert_str(&new_line);
                        }
                        self.block_insert_col += 1;
                        self.textarea.move_cursor(CursorMove::Jump(
                            min_row as u16,
                            self.block_insert_col as u16,
                        ));
                    }
                    ratatui::crossterm::event::KeyCode::Backspace => {
                        if self.block_insert_col > original_col {
                            self.block_insert_col -= 1;
                            for row in min_row..=max_row {
                                let line = self.textarea.lines()[row].clone();
                                let target_col = self.block_insert_col;
                                let start_byte = line
                                    .char_indices()
                                    .nth(target_col)
                                    .map(|(b, _)| b)
                                    .unwrap_or(line.len());
                                let mut new_line = line.clone();
                                if target_col < new_line.len() {
                                    new_line.remove(start_byte);
                                }
                                self.textarea.move_cursor(CursorMove::Jump(row as u16, 0));
                                self.textarea.delete_line_by_end();
                                self.textarea.insert_str(&new_line);
                            }
                            self.textarea.move_cursor(CursorMove::Jump(
                                min_row as u16,
                                self.block_insert_col as u16,
                            ));
                        }
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub fn render(
        &mut self,
        terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    ) -> Result<(), EditorError> {
        terminal.draw(|f| {
            if let Err(e) = self.draw(f) {
                self.status = format!("Render error: {}", e);
            }
        })?;
        Ok(())
    }

    fn draw(&mut self, f: &mut Frame) -> Result<(), EditorError> {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Editor, info, or list area
                Constraint::Length(1), // Status line
                Constraint::Length(1), // Command line or key sequence
            ])
            .split(f.area());

        match self.mode {
            Mode::Search => {
                let items: Vec<ListItem> = self
                    .search_state
                    .results
                    .iter()
                    .map(|(text, _)| ListItem::new(text.clone()))
                    .collect();
                let title = match self.search_state.search_type {
                    SearchType::Backlinks => "Backlinks",
                    SearchType::Tags => "Tags",
                    SearchType::Files => "Files",
                    SearchType::None => "Search",
                };
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(format!("{}: {}", title, self.search_state.query))
                            .style(Style::default().fg(Color::White)),
                    )
                    .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
                let popup_area = Rect {
                    x: chunks[0].x + 2,
                    y: chunks[0].y + 2,
                    width: 50,
                    height: (self.search_state.results.len().min(10) + 2) as u16,
                };
                f.render_stateful_widget(list, popup_area, &mut self.search_state.list_state);
            }
            Mode::TagFiles => {
                let items: Vec<ListItem> = self
                    .tag_files
                    .iter()
                    .map(|(path, _)| ListItem::new(path.clone()))
                    .collect();
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Files for Tag")
                            .style(Style::default().fg(Color::White)),
                    )
                    .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
                let popup_area = Rect {
                    x: chunks[0].x + 2,
                    y: chunks[0].y + 2,
                    width: 50,
                    height: (self.tag_files.len().min(10) + 2) as u16,
                };
                f.render_stateful_widget(list, popup_area, &mut self.tag_files_state);
            }
            Mode::Normal
            | Mode::Insert
            | Mode::Complete
            | Mode::Command
            | Mode::Visual
            | Mode::VisualBlock
            | Mode::BlockInsert => match self.view {
                View::Editor => {
                    // Apply syntax highlighting
                    let text = self.textarea.lines().join("\n");
                    let syntax = self
                        .syntax_set
                        .find_syntax_by_extension("md")
                        .unwrap_or_else(|| {
                            self.syntax_set.find_syntax_by_name("Markdown").unwrap()
                        });
                    let mut highlighter = HighlightLines::new(syntax, &self.theme);
                    let mut highlighted_lines = Vec::new();

                    // Get selection range for Visual/VisualBlock modes
                    let selection_range = match self.mode {
                        Mode::Visual | Mode::VisualBlock => self.visual_anchor.map(|anchor| {
                            let cursor = self.textarea.cursor();
                            let start_row = anchor.0.min(cursor.0);
                            let end_row = anchor.0.max(cursor.0);
                            let start_col = if anchor.0 == start_row {
                                anchor.1
                            } else {
                                cursor.1
                            };
                            let end_col = if anchor.0 == end_row {
                                anchor.1
                            } else {
                                cursor.1
                            };
                            ((start_row, start_col), (end_row, end_col))
                        }),
                        _ => None,
                    };

                    for (row, line) in LinesWithEndings::from(&text).enumerate() {
                        let ranges = highlighter
                            .highlight_line(line, &self.syntax_set)
                            .map_err(|e| EditorError::SyntaxHighlighting(e.to_string()))?;
                        let mut spans: Vec<Span> = Vec::new();
                        let mut col = 0;

                        for (style, text) in ranges {
                            let color = Color::Rgb(
                                style.foreground.r,
                                style.foreground.g,
                                style.foreground.b,
                            );
                            let text_len = text.chars().count();
                            let mut span_style = Style::default().fg(color);

                            // Apply selection styling if within range
                            if let Some(((start_row, start_col), (end_row, end_col))) =
                                selection_range
                            {
                                if (row > start_row || (row == start_row && col >= start_col))
                                    && (row < end_row || (row == end_row && col < end_col))
                                {
                                    span_style = span_style.bg(Color::LightBlue);
                                    // Selection highlight
                                }
                            }

                            spans.push(Span::styled(text.to_string(), span_style));
                            col += text_len;
                        }
                        highlighted_lines.push(Line::from(spans));
                    }

                    // Calculate scroll offset to keep cursor in view only at edges
                    let cursor_row = self.textarea.cursor().0;
                    let area_height = chunks[0].height.saturating_sub(2) as usize; // Subtract borders
                    let visible_lines = area_height.min(highlighted_lines.len());

                    // Only scroll if cursor is outside the visible viewport
                    if cursor_row < self.scroll_offset {
                        self.scroll_offset = cursor_row;
                    } else if cursor_row >= self.scroll_offset + visible_lines {
                        self.scroll_offset = cursor_row - (visible_lines - 1);
                    }

                    // Ensure scroll_offset doesn't exceed document bounds
                    self.scroll_offset = self
                        .scroll_offset
                        .min(highlighted_lines.len().saturating_sub(visible_lines));

                    // Slice the highlighted lines to display only the visible portion
                    let start_line = self.scroll_offset;
                    let end_line =
                        (self.scroll_offset + visible_lines).min(highlighted_lines.len());
                    let visible_text = highlighted_lines[start_line..end_line].to_vec();

                    // Render the text
                    let block = self.textarea.block().cloned().unwrap_or_default();
                    let paragraph = Paragraph::new(visible_text)
                        .block(block)
                        .style(self.textarea.style());
                    f.render_widget(paragraph, chunks[0]);

                    // Render custom cursor
                    let cursor_col = self.textarea.cursor().1;
                    if cursor_row >= self.scroll_offset
                        && cursor_row < self.scroll_offset + visible_lines
                    {
                        let screen_row = (cursor_row - self.scroll_offset) as u16;
                        let max_width = chunks[0].width.saturating_sub(2) as usize; // Subtract borders
                        let cursor_x = (cursor_col as u16).min(max_width as u16); // Clamp cursor x
                        let cursor_area = Rect {
                            x: chunks[0].x + 1 + cursor_x,
                            y: chunks[0].y + 1 + screen_row,
                            width: 1,
                            height: 1,
                        };
                        let cursor_span =
                            Span::styled(" ", Style::default().bg(Color::White).fg(Color::Black));
                        f.render_widget(Paragraph::new(cursor_span), cursor_area);
                    }

                    // Render completion popup if active
                    if self.completion_state.active && !self.completion_state.suggestions.is_empty()
                    {
                        let items: Vec<ListItem> = self
                            .completion_state
                            .suggestions
                            .iter()
                            .map(|s| ListItem::new(format!("{s}{}", " ".repeat(50))))
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
                                    .style(Style::default().fg(Color::White).bg(Color::Black)),
                            )
                            .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
                        let popup_width = 40;
                        let popup_height =
                            (self.completion_state.suggestions.len().min(5) + 2) as u16;
                        let popup_area = Rect {
                            x: chunks[0].x + chunks[0].width.saturating_sub(popup_width),
                            y: chunks[0].y + 1,
                            width: popup_width,
                            height: popup_height,
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
            },
        }

        let status = Paragraph::new(format!("-- {} --", self.status))
            .style(Style::default().fg(Color::Yellow));
        f.render_widget(status, chunks[1]);

        let command = Paragraph::new(match self.mode {
            Mode::Command => format!(":{}", self.command),
            Mode::Normal if !self.key_sequence.is_empty() => format!("{}", self.key_sequence),
            _ => String::new(),
        })
        .style(Style::default().fg(Color::White));
        f.render_widget(command, chunks[2]);
        Ok(())
    }
}
