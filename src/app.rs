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
use std::time::SystemTime;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
    util::LinesWithEndings,
};
use tui_textarea::{CursorMove, Input, Key, TextArea};

#[derive(PartialEq, Clone, Copy, Debug)]
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
    FileTree,
    FileTreeVisual,
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

#[derive(PartialEq, Clone, Debug)]
pub enum SortBy {
    Name,
    Modified,
}

#[derive(Clone)]
pub enum TreeNode {
    File(String), // relative path
    Dir {
        path: String,
        expanded: bool,
        children: Vec<TreeNode>,
    },
}

#[derive(Clone)]
pub struct TreeItem {
    display: String,
    path: String,
    is_dir: bool,
    depth: usize,
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum BufferMode {
    Copy,
    Cut,
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
    horizontal_scroll_offset: usize, // New: Horizontal scroll
    // File tree fields
    file_tree: Vec<TreeNode>,
    visible_items: Vec<TreeItem>,
    tree_state: ListState,
    sort_by: SortBy,
    sort_asc: bool,
    yanked_paths: Vec<String>,
    prev_mode: Option<Mode>,
    tree_visual_anchor: Option<usize>,
    tree_width_percent: u16,
    full_tree: bool,
    buffer_mode: Option<BufferMode>,
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
            scroll_offset: 0,            // Initialize scroll offset
            horizontal_scroll_offset: 0, // Initialize horizontal scroll
            file_tree: Vec::new(),
            visible_items: Vec::new(),
            tree_state: ListState::default(),
            sort_by: SortBy::Name,
            sort_asc: true,
            yanked_paths: Vec::new(),
            prev_mode: None,
            tree_visual_anchor: None,
            tree_width_percent: 20,
            full_tree: false,
            buffer_mode: None,
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
        // Extract file name from the path
        let file_name = Path::new(&wikilink)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| {
                if s.ends_with(".md") {
                    s.to_string()
                } else {
                    format!("{}.md", s)
                }
            })
            .unwrap_or_else(|| {
                if wikilink.ends_with(".md") {
                    wikilink.clone()
                } else {
                    format!("{}.md", wikilink)
                }
            });

        // Try to find the file by file_name in the database
        let file_result = {
            let mut stmt = self
                .db
                .prepare("SELECT id, path FROM files WHERE file_name = ?")?;
            stmt.query_row([&file_name], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
            })
        };

        let (file_id, path) = match file_result {
            Ok((id, path)) => (id, path),
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                // File doesn't exist; create it in the base_dir
                let path = format!("{}/{}", self.base_dir, wikilink); // Use original wikilink as path
                if let Some(parent) = Path::new(&path).parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(&path, "")?;
                let output = Command::new("markdown-scanner")
                    .arg(&path)
                    .arg(&self.base_dir)
                    .output()?;
                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
                    return Err(EditorError::Scanner(error_msg));
                }
                let mut stmt = self
                    .db
                    .prepare("SELECT id FROM files WHERE file_name = ?")?;
                let file_id = stmt
                    .query_row([&file_name], |row| row.get(0))
                    .map_err(|e| EditorError::Database(e))?;
                (file_id, path)
            }
            Err(e) => return Err(EditorError::Database(e)),
        };

        self.history.truncate(self.history_index + 1);
        self.history.push((path.clone(), file_id));
        self.history_index += 1;
        self.open_file(path, file_id)?;
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
                .set_selection_style(Style::default().bg(Color::LightBlue));
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
            "SELECT f.file_name, f.id FROM files f
             JOIN file_tags ft ON f.id = ft.file_id
             JOIN tags t ON ft.tag_id = t.id
             WHERE t.tag = ?",
        )?;
        let files = stmt
            .query_map([tag], |row| {
                let file_name: String = row.get(0)?;
                let file_id: i64 = row.get(1)?;
                Ok((file_name, file_id))
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
            if let Some((_file_name, file_id)) = self.tag_files.get(selected) {
                // Retrieve full path from database
                let path: String = self
                    .db
                    .query_row("SELECT path FROM files WHERE id = ?", [file_id], |row| {
                        row.get(0)
                    })
                    .map_err(|e| EditorError::Database(e))?;
                self.history.truncate(self.history_index + 1);
                self.history.push((path.clone(), *file_id));
                self.history_index += 1;
                self.open_file(path, *file_id)?;
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
        self.view = View::Editor;
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
            // Use file_name from files table for the current file
            let file_name: String = self
                .db
                .query_row(
                    "SELECT file_name FROM files WHERE id = ?",
                    [self.file_id],
                    |row| row.get(0),
                )
                .map_err(|e| EditorError::Database(e))?;
            file_name
        };

        let query = "SELECT DISTINCT f.file_name, f.id
                     FROM backlinks b
                     JOIN files f ON b.file_id = f.id
                     JOIN files fp ON b.backlink_id = fp.id
                     WHERE fp.file_name LIKE ? AND f.id != ?";
        let mut stmt = self.db.prepare(query)?;
        let results = stmt
            .query_map(params![format!("%{}%", target), self.file_id], |row| {
                let file_name: String = row.get(0)?;
                let file_id: i64 = row.get(1)?;
                Ok((file_name, Some(file_id)))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        self.search_state.results = results;
        self.search_state.list_state = ListState::default();
        if !self.search_state.results.is_empty() {
            self.search_state.list_state.select(Some(0));
        }
        Ok(())
    }

    fn search_tags(&mut self) -> Result<(), EditorError> {
        let query = "SELECT DISTINCT tag FROM tags WHERE tag LIKE ?";
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
        let query = "SELECT file_name, id FROM files WHERE file_name LIKE ?";
        let mut stmt = self.db.prepare(query)?;
        let search_pattern = if self.search_state.query.is_empty() {
            "%".to_string()
        } else {
            format!("%{}%", self.search_state.query)
        };
        let results = stmt
            .query_map(params![search_pattern], |row| {
                let file_name: String = row.get(0)?;
                let file_id: i64 = row.get(1)?;
                Ok((file_name, Some(file_id)))
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
            if let Some((file_name, file_id)) = self.search_state.results.get(selected).cloned() {
                match self.search_state.search_type {
                    SearchType::Backlinks | SearchType::Files => {
                        if let Some(file_id) = file_id {
                            // Retrieve full path from database
                            let path: String = self
                                .db
                                .query_row(
                                    "SELECT path FROM files WHERE id = ?",
                                    [file_id],
                                    |row| row.get(0),
                                )
                                .map_err(|e| EditorError::Database(e))?;
                            self.history.truncate(self.history_index + 1);
                            self.history.push((path.clone(), file_id));
                            self.history_index += 1;
                            self.open_file(path, file_id)?;
                        }
                    }
                    SearchType::Tags => {
                        self.load_tag_files(&file_name)?;
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
            self.status = "No result selected".to_string();
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

    fn build_root(&self) -> Vec<TreeNode> {
        let mut root = Vec::new();
        if let Ok(iter) = fs::read_dir(&self.base_dir) {
            for entry in iter {
                if let Ok(entry) = entry {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(".") {
                        continue;
                    }
                    let p = entry.path();
                    let rel = name;
                    if p.is_dir() {
                        root.push(TreeNode::Dir {
                            path: rel,
                            expanded: false,
                            children: Vec::new(),
                        });
                    } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                        root.push(TreeNode::File(rel));
                    }
                }
            }
        }
        let mut sorted = root;
        Self::sort_entries(
            &mut sorted,
            self.sort_by.clone(),
            self.sort_asc,
            &self.base_dir,
        );
        sorted
    }

    fn sort_nodes(nodes: &mut Vec<TreeNode>, sort_by: SortBy, sort_asc: bool, base_dir: &str) {
        Self::sort_entries(nodes, sort_by.clone(), sort_asc, base_dir);
        for node in nodes.iter_mut() {
            if let TreeNode::Dir { children, .. } = node {
                Self::sort_nodes(children, sort_by.clone(), sort_asc, base_dir);
            }
        }
    }

    fn build_tree_node(
        full_path: &Path,
        rel_path: &str,
        sort_by: SortBy,
        sort_asc: bool,
        base_dir: &str,
    ) -> Vec<TreeNode> {
        let mut entries = Vec::new();
        if let Ok(iter) = fs::read_dir(full_path) {
            for entry in iter {
                if let Ok(entry) = entry {
                    let name = entry.file_name().to_string_lossy().to_string();
                    if name.starts_with(".") {
                        continue;
                    }
                    let p = entry.path();
                    let rel = if rel_path.is_empty() {
                        name
                    } else {
                        format!("{}/{}", rel_path, name)
                    };
                    if p.is_dir() {
                        entries.push(TreeNode::Dir {
                            path: rel,
                            expanded: false,
                            children: Vec::new(),
                        });
                    } else if p.extension().and_then(|s| s.to_str()) == Some("md") {
                        entries.push(TreeNode::File(rel));
                    }
                }
            }
        }
        let mut sorted = entries;
        Self::sort_entries(&mut sorted, sort_by, sort_asc, base_dir);
        sorted
    }

    fn sort_entries(entries: &mut Vec<TreeNode>, sort_by: SortBy, sort_asc: bool, base_dir: &str) {
        entries.sort_by(|a, b| {
            let (name_a, is_dir_a) = match a {
                TreeNode::File(p) => (p.as_str(), false),
                TreeNode::Dir { path, .. } => (path.as_str(), true),
            };
            let (name_b, is_dir_b) = match b {
                TreeNode::File(p) => (p.as_str(), false),
                TreeNode::Dir { path, .. } => (path.as_str(), true),
            };
            if is_dir_a != is_dir_b {
                is_dir_b.cmp(&is_dir_a) // dirs first
            } else {
                if sort_by == SortBy::Name {
                    if sort_asc {
                        name_a.cmp(name_b)
                    } else {
                        name_b.cmp(name_a)
                    }
                } else {
                    let path_a = Path::new(base_dir).join(name_a);
                    let path_b = Path::new(base_dir).join(name_b);
                    let time_a = fs::metadata(&path_a)
                        .and_then(|m| m.modified())
                        .map_err(|e| {
                            eprintln!("Error getting metadata for {}: {}", path_a.display(), e)
                        })
                        .unwrap_or(SystemTime::UNIX_EPOCH);
                    let time_b = fs::metadata(&path_b)
                        .and_then(|m| m.modified())
                        .map_err(|e| {
                            eprintln!("Error getting metadata for {}: {}", path_b.display(), e)
                        })
                        .unwrap_or(SystemTime::UNIX_EPOCH);
                    if sort_asc {
                        time_a.cmp(&time_b)
                    } else {
                        time_b.cmp(&time_a)
                    }
                }
            }
        });
    }

    fn toggle_sort_modified(&mut self) {
        if self.sort_by == SortBy::Modified {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_by = SortBy::Modified; // Keep enum variant as Created for compatibility
            self.sort_asc = true;
        }
        self.status = format!(
            "Sorted by modification time ({})",
            if self.sort_asc {
                "ascending"
            } else {
                "descending"
            }
        );
    }

    fn update_tree_sort(&mut self) {
        Self::sort_nodes(
            &mut self.file_tree,
            self.sort_by.clone(),
            self.sort_asc,
            &self.base_dir,
        );
        self.update_visible();
    }

    fn toggle_sort_name(&mut self) {
        if self.sort_by == SortBy::Name {
            self.sort_asc = !self.sort_asc;
        } else {
            self.sort_by = SortBy::Name;
            self.sort_asc = true;
        }
    }

    fn update_visible(&mut self) {
        let mut visible = Vec::new();
        Self::add_nodes_to_visible(&self.file_tree, 0, &mut visible);
        self.visible_items = visible;
    }

    fn add_nodes_to_visible(nodes: &[TreeNode], depth: usize, visible: &mut Vec<TreeItem>) {
        for node in nodes {
            match node {
                TreeNode::File(p) => {
                    let display = format!(
                        "{}{} {}",
                        "  ".repeat(depth),
                        '',
                        Path::new(p)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                    );
                    visible.push(TreeItem {
                        display,
                        path: p.clone(),
                        is_dir: false,
                        depth,
                    });
                }
                TreeNode::Dir {
                    path,
                    expanded,
                    children,
                } => {
                    let display = format!(
                        "{}{} {}/",
                        "  ".repeat(depth),
                        '',
                        Path::new(path)
                            .file_name()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                    );
                    visible.push(TreeItem {
                        display,
                        path: path.clone(),
                        is_dir: true,
                        depth,
                    });
                    if *expanded {
                        Self::add_nodes_to_visible(children, depth + 1, visible);
                    }
                }
            }
        }
    }

    fn find_node_mut<'a>(
        nodes: &'a mut Vec<TreeNode>,
        path_segments: &[&str],
    ) -> Option<&'a mut TreeNode> {
        if path_segments.is_empty() {
            return None;
        }
        let mut current: &mut Vec<TreeNode> = nodes;
        for (i, &name) in path_segments.iter().enumerate() {
            let mut found_idx = None;
            for (idx, node) in current.iter_mut().enumerate() {
                let node_name = match node {
                    TreeNode::File(p) => Path::new(p.as_str())
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(""),
                    TreeNode::Dir { path, .. } => Path::new(path.as_str())
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or(""),
                };
                if node_name == name {
                    found_idx = Some(idx);
                    break;
                }
            }
            if let Some(idx) = found_idx {
                if i == path_segments.len() - 1 {
                    return Some(&mut current[idx]);
                } else {
                    if let TreeNode::Dir { children, .. } = &mut current[idx] {
                        current = children;
                    } else {
                        return None;
                    }
                }
            } else {
                return None;
            }
        }
        None
    }

    fn toggle_expand_dir(&mut self, index: usize) -> Result<(), EditorError> {
        if index >= self.visible_items.len() {
            return Ok(());
        }
        let item = self.visible_items[index].clone();
        if item.is_dir {
            let segments: Vec<&str> = item.path.split('/').collect();
            if let Some(node) = Self::find_node_mut(&mut self.file_tree, &segments) {
                if let TreeNode::Dir {
                    ref mut expanded,
                    ref mut children,
                    ref path,
                } = *node
                {
                    *expanded = !*expanded;
                    if *expanded && children.is_empty() {
                        let full = Path::new(&self.base_dir).join(path);
                        *children = Self::build_tree_node(
                            &full,
                            path.as_str(),
                            self.sort_by.clone(),
                            self.sort_asc,
                            &self.base_dir,
                        );
                    }
                    self.update_visible();
                }
            }
        }
        Ok(())
    }

    fn expand_dir(&mut self, index: usize) -> Result<(), EditorError> {
        if index >= self.visible_items.len() {
            return Ok(());
        }
        let item = self.visible_items[index].clone();
        if item.is_dir {
            let segments: Vec<&str> = item.path.split('/').collect();
            if let Some(node) = Self::find_node_mut(&mut self.file_tree, &segments) {
                if let TreeNode::Dir {
                    ref mut expanded,
                    ref mut children,
                    ref path,
                } = *node
                {
                    if !*expanded {
                        *expanded = true;
                        if children.is_empty() {
                            let full = Path::new(&self.base_dir).join(path);
                            *children = Self::build_tree_node(
                                &full,
                                path.as_str(),
                                self.sort_by.clone(),
                                self.sort_asc,
                                &self.base_dir,
                            );
                        }
                        self.update_visible();
                    }
                }
            }
        }
        Ok(())
    }

    fn collapse_dir(&mut self, index: usize) -> Result<(), EditorError> {
        if index >= self.visible_items.len() {
            return Ok(());
        }
        let item = self.visible_items[index].clone();
        if item.is_dir {
            let segments: Vec<&str> = item.path.split('/').collect();
            if let Some(node) = Self::find_node_mut(&mut self.file_tree, &segments) {
                if let TreeNode::Dir {
                    ref mut expanded, ..
                } = *node
                {
                    if *expanded {
                        *expanded = false;
                        self.update_visible();
                    }
                }
            }
        }
        Ok(())
    }

    fn delete_selected_file(&mut self) -> Result<(), EditorError> {
        if let Some(selected) = self.tree_state.selected() {
            let item = self.visible_items[selected].clone();
            if !item.is_dir {
                let full_path = Path::new(&self.base_dir)
                    .join(&item.path)
                    .to_string_lossy()
                    .to_string();
                fs::remove_file(&full_path)?;
                let output = Command::new("markdown-scanner")
                    .arg("--delete")
                    .arg(&full_path)
                    .arg(&self.base_dir)
                    .output()?;
                if !output.status.success() {
                    let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
                    return Err(EditorError::Scanner(error_msg));
                }
                self.remove_node(&item.path);
                self.update_visible();
            } else {
                self.status = "Cannot delete directories".to_string();
            }
        }
        Ok(())
    }

    fn delete_selected_files(&mut self) -> Result<(), EditorError> {
        let current = self.tree_state.selected().unwrap_or(0);
        let anchor = self.tree_visual_anchor.unwrap_or(current);
        let min = anchor.min(current);
        let max = anchor.max(current);
        let mut to_delete = Vec::new();
        for i in min..=max {
            let item = self.visible_items[i].clone();
            if !item.is_dir {
                to_delete.push(item.path);
            }
        }
        for path in to_delete {
            let full_path = Path::new(&self.base_dir)
                .join(&path)
                .to_string_lossy()
                .to_string();
            fs::remove_file(&full_path)?;
            let output = Command::new("markdown-scanner")
                .arg("--delete")
                .arg(&full_path)
                .arg(&self.base_dir)
                .output()?;
            if !output.status.success() {
                let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            self.remove_node(&path);
        }
        self.update_visible();
        self.tree_state.select(Some(min));
        Ok(())
    }

    fn remove_node(&mut self, path: &str) {
        let segments: Vec<&str> = path.split('/').collect();
        if segments.len() == 1 {
            self.file_tree.retain(|n| match n {
                TreeNode::File(p) => p.as_str() != path,
                TreeNode::Dir { path: d, .. } => d.as_str() != path,
            });
        } else {
            let parent_segments = &segments[0..segments.len() - 1];
            if let Some(parent) = Self::find_node_mut(&mut self.file_tree, parent_segments) {
                if let TreeNode::Dir {
                    ref mut children, ..
                } = *parent
                {
                    children.retain(|n| match n {
                        TreeNode::File(p) => {
                            p.rsplit('/').next().unwrap() != *segments.last().unwrap()
                        }
                        TreeNode::Dir { path: d, .. } => {
                            d.rsplit('/').next().unwrap() != *segments.last().unwrap()
                        }
                    });
                }
            }
        }
    }

    fn yank_selected(&mut self) {
        let current = self.tree_state.selected().unwrap_or(0);
        let anchor = self.tree_visual_anchor.unwrap_or(current);
        let min = anchor.min(current);
        let max = anchor.max(current);
        self.yanked_paths.clear();
        for i in min..=max {
            let item = &self.visible_items[i];
            if !item.is_dir {
                self.yanked_paths.push(item.path.clone());
            }
        }
        self.status = format!("Yanked {} paths", self.yanked_paths.len());
    }

    fn cut_selected(&mut self) {
        self.yank_selected();
        self.buffer_mode = Some(BufferMode::Cut);
        self.status = format!("Cut {} paths to buffer", self.yanked_paths.len());
    }

    fn copy_selected(&mut self) {
        self.yank_selected();
        self.buffer_mode = Some(BufferMode::Copy);
        self.status = format!("Copied {} paths to buffer", self.yanked_paths.len());
    }

    fn paste_buffer(&mut self) -> Result<(), EditorError> {
        if self.yanked_paths.is_empty() {
            self.status = "No paths in buffer".to_string();
            return Ok(());
        }
        if let Some(selected) = self.tree_state.selected() {
            let item = self.visible_items[selected].clone();
            let target_dir = if item.is_dir {
                item.path
            } else {
                Path::new(&item.path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .to_string()
            };
            match self.buffer_mode {
                Some(BufferMode::Cut) => {
                    self.move_paths(self.yanked_paths.clone(), target_dir)?;
                    self.yanked_paths.clear();
                    self.buffer_mode = None;
                    self.status = "Pasted (moved) from buffer".to_string();
                }
                Some(BufferMode::Copy) => {
                    self.copy_paths(self.yanked_paths.clone(), target_dir)?;
                    // Do not clear for copy, allow multiple pastes
                    self.status = "Pasted (cloned) from buffer".to_string();
                }
                None => {
                    self.status = "No buffer mode set".to_string();
                }
            }
            self.file_tree = self.build_root();
            self.update_visible();
        }
        Ok(())
    }

    fn create_new_file(&mut self, name: String) -> Result<(), EditorError> {
        if let Some(selected) = self.tree_state.selected() {
            let item = self.visible_items[selected].clone();
            let target_dir = if item.is_dir {
                item.path
            } else {
                Path::new(&item.path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .to_string()
            };
            let new_path = if target_dir.is_empty() {
                format!("{}.md", name)
            } else {
                format!("{}/{}.md", target_dir, name)
            };
            let full_path = Path::new(&self.base_dir)
                .join(&new_path)
                .to_string_lossy()
                .to_string();
            fs::write(&full_path, "")?;
            let output = Command::new("markdown-scanner")
                .arg(&full_path)
                .arg(&self.base_dir)
                .output()?;
            if !output.status.success() {
                let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            self.file_tree = self.build_root();
            self.update_visible();
            self.status = "Created new file".to_string();
        }
        Ok(())
    }

    fn rename_selected(&mut self, new_name: String) -> Result<(), EditorError> {
        if let Some(selected) = self.tree_state.selected() {
            let item = self.visible_items[selected].clone();
            if item.is_dir {
                self.status = "Cannot rename directories".to_string();
                return Ok(());
            }
            let old_path = item.path;
            let old_full = Path::new(&self.base_dir)
                .join(&old_path)
                .to_string_lossy()
                .to_string();
            let parent = Path::new(&old_path)
                .parent()
                .and_then(|p| p.to_str())
                .unwrap_or("")
                .to_string();
            let new_path = if parent.is_empty() {
                new_name.clone()
            } else {
                format!("{}/{}", parent, new_name)
            };
            let new_full = Path::new(&self.base_dir)
                .join(&new_path)
                .to_string_lossy()
                .to_string();
            fs::rename(&old_full, &new_full)?;
            let output_delete = Command::new("markdown-scanner")
                .arg("--delete")
                .arg(&old_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_delete.status.success() {
                let error_msg = String::from_utf8_lossy(&output_delete.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            let output_scan = Command::new("markdown-scanner")
                .arg(&new_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_scan.status.success() {
                let error_msg = String::from_utf8_lossy(&output_scan.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            self.remove_node(&old_path);
            self.file_tree = self.build_root();
            self.update_visible();
        }
        Ok(())
    }

    fn move_paths(&mut self, paths: Vec<String>, target_dir: String) -> Result<(), EditorError> {
        for old_path in paths {
            let old_full = Path::new(&self.base_dir)
                .join(&old_path)
                .to_string_lossy()
                .to_string();
            let filename = Path::new(&old_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let new_path = format!("{}/{}", target_dir, filename);
            let new_full = Path::new(&self.base_dir)
                .join(&new_path)
                .to_string_lossy()
                .to_string();
            fs::rename(&old_full, &new_full)?;
            let output_delete = Command::new("markdown-scanner")
                .arg("--delete")
                .arg(&old_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_delete.status.success() {
                let error_msg = String::from_utf8_lossy(&output_delete.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            let output_scan = Command::new("markdown-scanner")
                .arg(&new_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_scan.status.success() {
                let error_msg = String::from_utf8_lossy(&output_scan.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            self.remove_node(&old_path);
        }
        Ok(())
    }

    fn copy_paths(&mut self, paths: Vec<String>, target_dir: String) -> Result<(), EditorError> {
        for old_path in paths {
            let old_full = Path::new(&self.base_dir)
                .join(&old_path)
                .to_string_lossy()
                .to_string();
            let filename = Path::new(&old_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
            let new_path = format!("{}/{}", target_dir, filename);
            let new_full = Path::new(&self.base_dir)
                .join(&new_path)
                .to_string_lossy()
                .to_string();
            fs::copy(&old_full, &new_full)?;
            let output_scan = Command::new("markdown-scanner")
                .arg(&new_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_scan.status.success() {
                let error_msg = String::from_utf8_lossy(&output_scan.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
        }
        Ok(())
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
                            "\\t" => {
                                if self.file_tree.is_empty() {
                                    self.file_tree = self.build_root();
                                }
                                self.update_visible();
                                if !self.visible_items.is_empty() {
                                    self.tree_state.select(Some(0));
                                }
                                self.mode = Mode::FileTree;
                                self.key_sequence.clear();
                                self.status = "Entered File Tree mode".to_string();
                            }
                            s if !("\\ob".starts_with(s)
                                || "\\ot".starts_with(s)
                                || "\\f".starts_with(s)
                                || "\\oot".starts_with(s)
                                || "\\ooy".starts_with(s)
                                || "\\ooT".starts_with(s)
                                || "\\t".starts_with(s)) =>
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
                        self.prev_mode = Some(Mode::Normal);
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
                    let mode = self.prev_mode.unwrap_or(Mode::Normal);
                    self.mode = mode;
                    self.status = "Normal".to_string();
                    self.command.clear();
                    self.prev_mode = None;
                }
                ratatui::crossterm::event::KeyCode::Enter => {
                    if self.command == "w" {
                        self.save_file()?;
                    } else if self.command == "q" {
                        self.should_quit = true;
                    } else if self.command == "wq" {
                        self.save_file()?;
                        self.should_quit = true;
                    } else if self.command.starts_with("rename ") {
                        let new_name = self.command.trim_start_matches("rename ").to_string();
                        self.rename_selected(new_name)?;
                    } else if self.command.starts_with("new ") {
                        let name = self.command.trim_start_matches("new ").to_string();
                        self.create_new_file(name)?;
                    } else {
                        self.status = format!("Unknown command: {}", self.command);
                    }
                    let mode = self.prev_mode.unwrap_or(Mode::Normal);
                    self.mode = mode;
                    self.command.clear();
                    self.prev_mode = None;
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
                                        .title("Midetor")
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
                let _input = Input::from(event);
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
                            let char_count = line.chars().count();
                            let mut new_line = line.clone();
                            if target_col > char_count {
                                new_line.push_str(&" ".repeat(target_col - char_count));
                            }
                            let start_byte = new_line
                                .char_indices()
                                .nth(target_col)
                                .map(|(b, _)| b)
                                .unwrap_or(new_line.len());
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
                                let char_count = new_line.chars().count();
                                if target_col < char_count {
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
            Mode::FileTree => {
                if !self.key_sequence.is_empty() {
                    if let ratatui::crossterm::event::KeyCode::Char(c) = event.code {
                        self.key_sequence.push(c);
                        let sequence = self.key_sequence.clone();
                        match sequence.as_str() {
                            "oc" => {
                                self.toggle_sort_modified();
                                self.update_tree_sort();
                                self.key_sequence.clear();
                                self.status = format!(
                                    "Sorted by creation time ({})",
                                    if self.sort_asc {
                                        "ascending"
                                    } else {
                                        "descending"
                                    }
                                );
                            }
                            "on" => {
                                self.toggle_sort_name();
                                self.update_tree_sort();
                                self.key_sequence.clear();
                                self.status = format!(
                                    "Sorted by name ({})",
                                    if self.sort_asc {
                                        "ascending"
                                    } else {
                                        "descending"
                                    }
                                );
                            }
                            s if !s.starts_with("oc") && !s.starts_with("on") => {
                                self.key_sequence.clear();
                                self.status = format!("Invalid sequence: {}", s);
                            }
                            _ => {}
                        }
                        return Ok(());
                    }
                }
                match event.code {
                    ratatui::crossterm::event::KeyCode::Esc => {
                        self.mode = Mode::Normal;
                        self.status = "Normal".to_string();
                    }
                    ratatui::crossterm::event::KeyCode::Up => {
                        let selected = self.tree_state.selected().unwrap_or(0);
                        if selected > 0 {
                            self.tree_state.select(Some(selected - 1));
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Down => {
                        let selected = self.tree_state.selected().unwrap_or(0);
                        if selected < self.visible_items.len() - 1 {
                            self.tree_state.select(Some(selected + 1));
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Left => {
                        if let Some(selected) = self.tree_state.selected() {
                            let item = &self.visible_items[selected];
                            if item.is_dir {
                                self.collapse_dir(selected)?;
                            } else {
                                let depth = item.depth;
                                for i in (0..selected).rev() {
                                    if self.visible_items[i].depth < depth {
                                        self.tree_state.select(Some(i));
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Right => {
                        if let Some(selected) = self.tree_state.selected() {
                            let item = &self.visible_items[selected];
                            if item.is_dir {
                                self.expand_dir(selected)?;
                            }
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Enter => {
                        if let Some(selected) = self.tree_state.selected() {
                            let item = self.visible_items[selected].clone();
                            if item.is_dir {
                                self.toggle_expand_dir(selected)?;
                            } else {
                                self.open_wikilink_file(item.path)?;
                                // self.mode = Mode::Normal;
                            }
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Char('v') => {
                        if let Some(selected) = self.tree_state.selected() {
                            self.tree_visual_anchor = Some(selected);
                            self.mode = Mode::FileTreeVisual;
                            self.status = "Visual".to_string();
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Char('d') => {
                        self.delete_selected_file()?;
                    }
                    ratatui::crossterm::event::KeyCode::Char('r') => {
                        self.prev_mode = Some(Mode::FileTree);
                        self.mode = Mode::Command;
                        self.command = "rename ".to_string();
                        self.status = "Rename to:".to_string();
                    }
                    ratatui::crossterm::event::KeyCode::Char('n') => {
                        self.prev_mode = Some(Mode::FileTree);
                        self.mode = Mode::Command;
                        self.command = "new ".to_string();
                        self.status = "New file name:".to_string();
                    }
                    ratatui::crossterm::event::KeyCode::Char('y') => {
                        self.copy_selected();
                    }
                    ratatui::crossterm::event::KeyCode::Char('x') => {
                        self.cut_selected();
                    }
                    ratatui::crossterm::event::KeyCode::Char('p') => {
                        self.paste_buffer()?;
                    }
                    ratatui::crossterm::event::KeyCode::Char('<') => {
                        if !self.full_tree && self.tree_width_percent > 10 {
                            self.tree_width_percent -= 5;
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Char('>') => {
                        if !self.full_tree && self.tree_width_percent < 50 {
                            self.tree_width_percent += 5;
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Char('f') => {
                        self.full_tree = !self.full_tree;
                        self.status = if self.full_tree {
                            "Full-screen FileTree"
                        } else {
                            "Split FileTree"
                        }
                        .to_string();
                    }
                    ratatui::crossterm::event::KeyCode::Char(c) => {
                        self.key_sequence.push(c);
                    }
                    _ => {}
                }
            }
            Mode::FileTreeVisual => match event.code {
                ratatui::crossterm::event::KeyCode::Esc => {
                    self.tree_visual_anchor = None;
                    self.mode = Mode::FileTree;
                    self.status = "File Tree".to_string();
                }
                ratatui::crossterm::event::KeyCode::Up => {
                    let selected = self.tree_state.selected().unwrap_or(0);
                    if selected > 0 {
                        self.tree_state.select(Some(selected - 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Down => {
                    let selected = self.tree_state.selected().unwrap_or(0);
                    if selected < self.visible_items.len() - 1 {
                        self.tree_state.select(Some(selected + 1));
                    }
                }
                ratatui::crossterm::event::KeyCode::Char('d') => {
                    self.delete_selected_files()?;
                    self.tree_visual_anchor = None;
                    self.mode = Mode::FileTree;
                }
                ratatui::crossterm::event::KeyCode::Char('y') => {
                    self.copy_selected();
                    self.tree_visual_anchor = None;
                    self.mode = Mode::FileTree;
                }
                ratatui::crossterm::event::KeyCode::Char('x') => {
                    self.cut_selected();
                    self.tree_visual_anchor = None;
                    self.mode = Mode::FileTree;
                }
                ratatui::crossterm::event::KeyCode::Char('r') => {
                    let current = self.tree_state.selected().unwrap_or(0);
                    let anchor = self.tree_visual_anchor.unwrap_or(current);
                    if anchor == current {
                        self.prev_mode = Some(Mode::FileTreeVisual);
                        self.mode = Mode::Command;
                        self.command = "rename ".to_string();
                        self.status = "Rename to:".to_string();
                    } else {
                        self.status = "Rename only for single file".to_string();
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
                // Render search input field or results list
                let title = match self.search_state.search_type {
                    SearchType::Backlinks => format!("Backlinks: {}", self.search_state.query),
                    SearchType::Tags => format!("Tags: {}", self.search_state.query),
                    SearchType::Files => format!("Files: {}", self.search_state.query),
                    SearchType::None => "Search".to_string(),
                };
                if self.search_state.results.is_empty() && self.search_state.query.is_empty() {
                    // Render input field for search
                    let input = Paragraph::new(self.search_state.query.clone())
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(title)
                                .style(Style::default().fg(Color::White)),
                        )
                        .style(Style::default().fg(Color::Yellow));
                    f.render_widget(input, chunks[0]);
                } else {
                    // Render search results
                    let items: Vec<ListItem> = self
                        .search_state
                        .results
                        .iter()
                        .map(|(text, _)| ListItem::new(text.clone()))
                        .collect();
                    let list = List::new(items)
                        .block(
                            Block::default()
                                .borders(Borders::ALL)
                                .title(title)
                                .style(Style::default().fg(Color::White)),
                        )
                        .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
                    f.render_stateful_widget(list, chunks[0], &mut self.search_state.list_state);
                }
            }
            Mode::TagFiles => {
                let items: Vec<ListItem> = self
                    .tag_files
                    .iter()
                    .map(|(file_name, _)| ListItem::new(file_name.clone()))
                    .collect();
                let list = List::new(items)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title("Files for Tag")
                            .style(Style::default().fg(Color::White)),
                    )
                    .highlight_style(Style::default().bg(Color::White).fg(Color::Black));
                f.render_stateful_widget(list, chunks[0], &mut self.tag_files_state);
            }
            Mode::FileTree | Mode::FileTreeVisual => {
                let tree_constraint = if self.full_tree {
                    Constraint::Percentage(100)
                } else {
                    Constraint::Percentage(self.tree_width_percent)
                };
                let editor_constraint = if self.full_tree {
                    Constraint::Length(0)
                } else {
                    Constraint::Percentage(100 - self.tree_width_percent)
                };
                let main_chunks = Layout::default()
                    .direction(Direction::Horizontal)
                    .constraints([tree_constraint, editor_constraint])
                    .split(chunks[0]);

                let visual_min = self
                    .tree_visual_anchor
                    .map(|a| a.min(self.tree_state.selected().unwrap_or(0)))
                    .unwrap_or(usize::MAX);
                let visual_max = self
                    .tree_visual_anchor
                    .map(|a| a.max(self.tree_state.selected().unwrap_or(0)))
                    .unwrap_or(0);

                let items: Vec<ListItem> = self
                    .visible_items
                    .iter()
                    .enumerate()
                    .map(|(i, item)| {
                        let base_style = if item.is_dir {
                            Style::default().fg(Color::LightBlue)
                        } else {
                            Style::default().fg(Color::White)
                        };
                        let mut li = ListItem::new(item.display.clone()).style(base_style);
                        if self.mode == Mode::FileTreeVisual && i >= visual_min && i <= visual_max {
                            li = li.style(Style::default().bg(Color::LightBlue));
                        } else if Some(i) == self.tree_state.selected() {
                            li = li.style(Style::default().bg(Color::White).fg(Color::Black));
                        }
                        li
                    })
                    .collect();

                let list = List::new(items).block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("File Tree")
                        .style(Style::default().fg(Color::White)),
                );
                f.render_stateful_widget(list, main_chunks[0], &mut self.tree_state);

                if !self.full_tree {
                    self.render_editor(f, main_chunks[1])?;
                }
            }
            Mode::Normal
            | Mode::Insert
            | Mode::Complete
            | Mode::Command
            | Mode::Visual
            | Mode::VisualBlock
            | Mode::BlockInsert => match self.view {
                View::Editor => {
                    self.render_editor(f, chunks[0])?;
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
            Mode::Search => format!("/{}", self.search_state.query),
            Mode::Normal | Mode::FileTree if !self.key_sequence.is_empty() => {
                format!("{}", self.key_sequence)
            }
            _ => String::new(),
        })
        .style(Style::default().fg(Color::White));
        f.render_widget(command, chunks[2]);
        Ok(())
    }

    fn render_editor(&mut self, f: &mut Frame, area: Rect) -> Result<(), EditorError> {
        // Apply syntax highlighting
        let text = self.textarea.lines().join("\n");
        let syntax = self
            .syntax_set
            .find_syntax_by_extension("md")
            .unwrap_or_else(|| self.syntax_set.find_syntax_by_name("Markdown").unwrap());
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
                let color = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let text_len = text.chars().count();
                let mut span_style = Style::default().fg(color);

                // Apply selection styling if within range
                if let Some(((start_row, start_col), (end_row, end_col))) = selection_range {
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

        // Calculate scroll offsets
        let cursor_row = self.textarea.cursor().0;
        let cursor_col = self.textarea.cursor().1;
        let area_height = area.height.saturating_sub(2) as usize; // Subtract borders
        let area_width = area.width.saturating_sub(2) as usize; // Subtract borders
        let visible_lines = area_height.min(highlighted_lines.len());

        // Vertical scrolling: only when cursor is outside visible area
        if cursor_row < self.scroll_offset {
            self.scroll_offset = cursor_row;
        } else if cursor_row >= self.scroll_offset + visible_lines {
            self.scroll_offset = cursor_row - (visible_lines - 1);
        }
        self.scroll_offset = self
            .scroll_offset
            .min(highlighted_lines.len().saturating_sub(visible_lines));

        // Horizontal scrolling: only when cursor is outside visible width
        if cursor_col < self.horizontal_scroll_offset {
            self.horizontal_scroll_offset = cursor_col;
        } else if cursor_col >= self.horizontal_scroll_offset + area_width {
            self.horizontal_scroll_offset = cursor_col - (area_width - 1);
        }

        // Slice lines and apply horizontal scrolling
        let start_line = self.scroll_offset;
        let end_line = (self.scroll_offset + visible_lines).min(highlighted_lines.len());
        let mut visible_text = Vec::new();
        for line in highlighted_lines[start_line..end_line].iter() {
            let mut new_spans = Vec::new();
            let mut col = 0;
            for span in &line.spans {
                let text = span.content.as_ref();
                let span_len = text.chars().count();
                let span_start = col;
                let span_end = col + span_len;

                if span_end > self.horizontal_scroll_offset {
                    let start_char = if span_start < self.horizontal_scroll_offset {
                        self.horizontal_scroll_offset - span_start
                    } else {
                        0
                    };
                    let text_chars: Vec<char> = text.chars().collect();
                    let sliced_text = text_chars[start_char..].iter().collect::<String>();
                    if !sliced_text.is_empty() {
                        new_spans.push(Span::styled(sliced_text, span.style));
                    }
                }
                col += span_len;
            }
            visible_text.push(Line::from(new_spans));
        }

        // Render the text
        let block = self.textarea.block().cloned().unwrap_or_default();
        let paragraph = Paragraph::new(visible_text)
            .block(block)
            .style(self.textarea.style());
        f.render_widget(paragraph, area);

        // Render custom cursor
        if cursor_row >= self.scroll_offset && cursor_row < self.scroll_offset + visible_lines {
            let screen_row = (cursor_row - self.scroll_offset) as u16;
            let screen_col = (cursor_col.saturating_sub(self.horizontal_scroll_offset)) as u16;
            let max_width = area_width as u16;
            let cursor_x = screen_col.min(max_width); // Clamp cursor x
            let cursor_area = Rect {
                x: area.x + 1 + cursor_x,
                y: area.y + 1 + screen_row,
                width: 1,
                height: 1,
            };

            // Get the actual char at cursor (or space if beyond line end)
            let line = self
                .textarea
                .lines()
                .get(cursor_row)
                .cloned()
                .unwrap_or_default();
            let ch: char = line.chars().nth(cursor_col).unwrap_or(' ');

            // Render the char with inverted colors
            let cursor_style = Style::default().bg(Color::White).fg(Color::Black);
            let cursor_span = Span::styled(ch.to_string(), cursor_style);
            f.render_widget(Paragraph::new(cursor_span), cursor_area);
        }

        // Render completion popup if active
        if self.completion_state.active && !self.completion_state.suggestions.is_empty() {
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
            let popup_height = (self.completion_state.suggestions.len().min(5) + 2) as u16;
            let popup_area = Rect {
                x: area.x + area.width.saturating_sub(popup_width),
                y: area.y + 1,
                width: popup_width,
                height: popup_height,
            };
            f.render_stateful_widget(list, popup_area, &mut self.completion_state.list_state);
        }
        Ok(())
    }
}
