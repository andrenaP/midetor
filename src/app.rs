use crate::error::EditorError;
use chrono::Local;
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Margin, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};
use rusqlite::Connection;
use rusqlite::params;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;
use syntect::{
    easy::HighlightLines,
    highlighting::{Theme, ThemeSet},
    parsing::SyntaxSet,
};
use tui_textarea::{CursorMove, Input, Key, TextArea};

use image::DynamicImage;
use ratatui_image::{FilterType, Resize, picker::Picker, protocol::StatefulProtocol};

use std::collections::HashMap;
use std::process::Child;
use std::process::Stdio;
use walkdir::WalkDir;

use unicode_width::UnicodeWidthChar;

use crate::lua_api::{EditorCommand, EditorContext, LuaEditorAPI};
use mlua::Lua;
use std::cell::RefCell;
use std::rc::Rc;

macro_rules! set_textarea_delafult_style {
    ($textarea:expr) => {
        $textarea.set_block(
            Block::default()
                .borders(Borders::ALL)
                .title("Midetor")
                .style(Style::default().fg(Color::White)),
        );

        $textarea.set_cursor_line_style(Style::default());
        $textarea.set_cursor_style(Style::default().bg(Color::White).fg(Color::Black));
        $textarea.set_selection_style(Style::default().bg(Color::LightBlue));
    };
}
macro_rules! gettitle {
    ($file_path:expr ) => {
        Path::new($file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string()
    };
}

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
    Variable,
}

#[derive(PartialEq, Clone, Debug)]
pub enum SearchType {
    None,
    Backlinks,
    Tags,
    Files,
    CustomSql,
    CustomLua { provider: String, on_select: String },
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
    music_path: String,
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
    image_picker: Option<Picker>,
    image_protocol: Option<StatefulProtocol>, // Use enum directly, no Box<dyn ...>
    current_image: Option<DynamicImage>,
    current_image_line: Option<usize>, // Line number of the current image reference
    current_image_index: usize,
    image_paths: Vec<(String, usize)>, // Cached image paths and their line numbers
    last_wikilink: Option<String>,     // Last processed wikilink to avoid redundant loads
    image_full_screen: bool,           // Toggle for full-screen image
    last_image_area: Option<Rect>,     // Track last render area to regenerate protocol
    audio_child: Option<Child>,
    read_mode: bool,
    yt_videos: HashMap<String, String>,
    editor_width: u16,
    visual_scroll_y: u16,
    lua: Lua,
    command_queue: Rc<RefCell<Vec<EditorCommand>>>,
    normal_keymaps: Rc<RefCell<HashMap<String, mlua::RegistryKey>>>,
    visual_keymaps: Rc<RefCell<HashMap<String, mlua::RegistryKey>>>,
    shared_context: Rc<RefCell<EditorContext>>,
    virtual_texts: HashMap<usize, Vec<(usize, String, String)>>,
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
    pub fn new(file_path: &str, base_dir: &str, music_path: &str) -> Result<Self, EditorError> {
        let db = Connection::open("markdown_data.db")?;
        db.execute("PRAGMA foreign_keys = ON;", [])?;

        let content = fs::read_to_string(file_path).unwrap_or_default();
        let mut textarea = TextArea::new(content.lines().map(|s| s.to_string()).collect());
        set_textarea_delafult_style!(textarea);
        let file_id = App::get_file_id(&db, file_path)?;
        let tags = App::load_tags(&db, file_id)?;
        let backlinks = App::load_backlinks(&db, file_id)?;

        // Initialize syntax highlighting
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults(); // Load ThemeSet
        let theme = theme_set.themes["base16-eighties.dark"].clone();
        // let highlighter = HighlightLines::new(syntax, &theme); // Use reference to boxed Theme
        let picker = Picker::from_query_stdio().map_err(|e| {
            EditorError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to initialize image picker: {}", e),
            ))
        })?;

        // Optionally load an initial image (e.g., if file_path is an image or referenced in Markdown)
        // let (image_protocol, current_image) = (None, None);
        //
        //
        //
        //

        // 1. Initialize Lua and shared state
        let lua = Lua::new();
        let command_queue = Rc::new(RefCell::new(Vec::new()));
        let normal_keymaps = Rc::new(RefCell::new(HashMap::new()));
        let visual_keymaps = Rc::new(RefCell::new(HashMap::new()));
        let shared_context = Rc::new(RefCell::new(EditorContext::default()));

        let api = LuaEditorAPI {
            command_queue: Rc::clone(&command_queue),
            normal_keymaps: Rc::clone(&normal_keymaps),
            visual_keymaps: Rc::clone(&visual_keymaps),
            context: Rc::clone(&shared_context),
        };

        // 2. Expose the API to Lua under the global variable `editor`
        lua.globals()
            .set("editor", api)
            .expect("Failed to set Lua globals");

        // 3. Load the user's config file
        if let Ok(config) = std::fs::read_to_string("init.lua") {
            if let Err(e) = lua.load(&config).exec() {
                eprintln!("Lua Error: {}", e);
            }
        }

        let mut app = App {
            db,
            file_path: file_path.to_string(),
            base_dir: base_dir.to_string(),
            music_path: music_path.to_string(),
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
            sort_by: SortBy::Modified,
            sort_asc: false,
            yanked_paths: Vec::new(),
            prev_mode: None,
            tree_visual_anchor: None,
            tree_width_percent: 40,
            full_tree: true,
            buffer_mode: None,
            image_picker: Some(picker),
            image_protocol: None,
            current_image: None,
            current_image_line: None,
            current_image_index: 0,
            image_paths: Vec::new(),
            last_wikilink: None,
            image_full_screen: false,
            last_image_area: None,
            audio_child: None,
            read_mode: false,
            yt_videos: HashMap::new(),
            editor_width: 0,
            visual_scroll_y: 0,
            lua,
            command_queue,
            normal_keymaps,
            visual_keymaps,
            shared_context,
            virtual_texts: HashMap::new(),
        };
        app.open_file(file_path.to_string(), file_id)?;

        Ok(app)
    }

    pub fn handle_paste(&mut self, text: String) -> Result<(), EditorError> {
        match self.mode {
            Mode::Normal => {
                self.textarea.insert_str(&text);
            }
            Mode::Insert => {
                self.textarea.insert_str(&text);
            }
            Mode::Command => {
                self.command.push_str(&text);
            }
            Mode::Search => {
                self.search_state.query.push_str(&text);
                self.update_search_results()?;
            }
            Mode::BlockInsert => {
                self.textarea.insert_str(&text);
                self.status = "Pasted (simple) in BlockInsert".to_string();
            }
            _ => {}
        }
        Ok(())
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
        set_textarea_delafult_style!(textarea);
        textarea.move_cursor(tui_textarea::CursorMove::Jump(0, 0));
        while textarea.undo() {}
        self.textarea = textarea;
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
        self.current_image_index = 0;
        self.image_paths = self.extract_image_paths();
        self.last_wikilink = None;

        self.yt_videos.clear();
        let query = "SELECT json_extract(metadata, '$.ytVideos') FROM files WHERE path = ?1";

        if let Ok(Some(json_str)) = self.db.query_row(query, params![path], |row| {
            row.get::<usize, Option<String>>(0)
        }) {
            // Parse as an array of JSON objects instead of tuples
            match serde_json::from_str::<Vec<HashMap<String, String>>>(&json_str) {
                Ok(videos) => {
                    for video in videos {
                        // Safely extract the "url" and "title" keys from the JSON object
                        if let (Some(url), Some(title)) = (video.get("url"), video.get("title")) {
                            self.yt_videos.insert(url.trim().to_string(), title.clone());
                        }
                    }
                }
                Err(e) => {
                    self.status = format!("JSON parse error: {}", e);
                }
            }
        }
        // Load image at cursor or first image
        self.load_image_at_cursor()?;

        Ok(())
    }

    fn open_wikilink_file(&mut self, wikilink: String) -> Result<(), EditorError> {
        // Extract file name from the path
        let wikilink = if wikilink.ends_with(".md") {
            wikilink
        } else {
            format!("{}.md", wikilink)
        };
        let file_name = Path::new(&wikilink)
            .file_name()
            .and_then(|s| s.to_str())
            .map(|s| {
                // if s.ends_with(".md") {
                s.to_string()
                // } else {
                //     format!("{}.md", s)
                // }
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
                // Only write an empty file if it doesn't already exist
                if !Path::new(&path).exists() {
                    fs::write(&path, "")?;
                }
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
        let (current_row, current_col) = self.textarea.cursor();
        let line = self.textarea.lines()[current_row].clone();

        // If a specific index from the old logic is passed, use it.
        // Otherwise (when index is usize::MAX), extract the link from the cursor position.
        let wikilink = if index < self.backlinks.len() {
            self.backlinks[index].0.clone()
        } else {
            self.extract_wikilink(&line, current_col).ok_or_else(|| {
                EditorError::InvalidBacklink("No valid wikilink found at cursor".to_string())
            })?
        };

        // Clean incomplete autocompletions
        if line.contains("[[") && !line.contains("]]") {
            let mut new_lines = self.textarea.lines().to_vec();
            new_lines[current_row] = line[..line.rfind("[[").unwrap_or(line.len())].to_string();
            self.textarea = TextArea::new(new_lines);
            set_textarea_delafult_style!(self.textarea);
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

    fn echo(&mut self, message: &str) -> Result<(), EditorError> {
        self.status = format!("{}", message);
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

        let query = match self.completion_state.completion_type {
            CompletionType::File => line
                .get(..col_bytes)
                .and_then(|s| s.rfind("[["))
                .map(|start| line[start + 2..col_bytes].to_string())
                .unwrap_or_default(),
            CompletionType::Tag => line
                .get(..col_bytes)
                .and_then(|s| s.rfind("#"))
                .map(|start| line[start + 1..col_bytes].to_string())
                .unwrap_or_default(),
            CompletionType::Variable => line
                .get(..col_bytes)
                .and_then(|s| s.rfind("@"))
                .map(|start| line[start + 1..col_bytes].to_string())
                .unwrap_or_default(),
            CompletionType::None => return Ok(()),
        };

        self.completion_state.query = query.clone();
        self.completion_state.suggestions = match self.completion_state.completion_type {
            CompletionType::File => {
                let search_pattern = format!("%{}%", query);
                let sql = "SELECT DISTINCT result FROM (
                    SELECT file_name AS result FROM files WHERE file_name LIKE ?
                    UNION
                    SELECT backlink AS result FROM backlinks WHERE backlink LIKE ?
                ) LIMIT 10";
                let mut stmt = self.db.prepare(sql)?;
                let closure = |row: &rusqlite::Row| row.get::<_, String>(0);
                let mapped_rows =
                    stmt.query_map(params![search_pattern, search_pattern], closure)?;
                mapped_rows.collect::<Result<Vec<_>, _>>()?
            }
            CompletionType::Tag => {
                let search_pattern = format!("%{}%", query);
                let sql = "SELECT tag FROM tags WHERE tag LIKE ? LIMIT 10";
                let mut stmt = self.db.prepare(sql)?;
                let closure = |row: &rusqlite::Row| row.get::<_, String>(0);
                let mapped_rows = stmt.query_map(params![search_pattern], closure)?;
                mapped_rows.collect::<Result<Vec<_>, _>>()?
            }
            CompletionType::Variable => {
                // Try to call a global Lua function named 'on_autocomplete'
                let globals = self.lua.globals();
                if let Ok(func) = globals.get::<_, mlua::Function>("on_autocomplete") {
                    // Pass the trigger type and current query to Lua
                    match func.call::<_, Vec<String>>(("@", query.clone())) {
                        Ok(suggestions) => suggestions,
                        Err(e) => {
                            self.status = format!("Lua Autocomplete Error: {}", e);
                            Vec::new()
                        }
                    }
                } else {
                    // Fallback if the Lua function isn't defined
                    Vec::new()
                }
            }
            CompletionType::None => Vec::new(),
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
            if let Some(suggestion) = self.completion_state.suggestions.get(selected).cloned() {
                let (current_row, current_col) = self.textarea.cursor();
                let mut current_lines = self.textarea.lines().to_vec();

                if let Some(original_line) = current_lines.get(current_row) {
                    // Work with a cloned, owned version of the line
                    let current_line_owned = original_line.clone();

                    let (trigger, _trigger_len) = match self.completion_state.completion_type {
                        CompletionType::File => ("[[", 2),
                        CompletionType::Tag => ("#", 1),
                        CompletionType::Variable => ("@", 1),
                        _ => ("", 0),
                    };

                    // Get the character indices up to the current cursor position.
                    let char_indices: Vec<(usize, char)> = current_line_owned
                        .char_indices()
                        .take(current_col)
                        .collect();

                    // Find the byte index of the trigger by iterating backwards
                    let trigger_start_byte_option = char_indices
                        .iter()
                        .rev()
                        .find(|(i, _)| current_line_owned[*i..].starts_with(trigger))
                        .map(|(i, _)| *i);

                    if let Some(start_byte) = trigger_start_byte_option {
                        let prefix_text = current_line_owned[..start_byte].to_owned();
                        let suffix_start_byte = current_line_owned
                            .char_indices()
                            .nth(current_col)
                            .map(|(i, _)| i)
                            .unwrap_or(current_line_owned.len());
                        let suffix_text = current_line_owned[suffix_start_byte..].to_owned();

                        let insert_text = match self.completion_state.completion_type {
                            CompletionType::File => format!("[[{}]]", suggestion),
                            CompletionType::Tag => format!("#{}", suggestion),
                            // Replace the old CompletionType::Variable logic in select_completion with this:
                            CompletionType::Variable => {
                                let globals = self.lua.globals();
                                if let Ok(func) =
                                    globals.get::<_, mlua::Function>("expand_autocomplete")
                                {
                                    // Pass the trigger ("@") and the selected suggestion name
                                    match func.call::<_, String>(("@", suggestion.clone())) {
                                        Ok(expanded_text) => expanded_text,
                                        Err(e) => {
                                            self.status = format!("Lua Snippet Error: {}", e);
                                            suggestion // Fallback to literal name if Lua fails
                                        }
                                    }
                                } else {
                                    suggestion // Fallback if Lua function doesn't exist
                                }
                            }
                            _ => suggestion,
                        };

                        let new_line = format!("{}{}{}", prefix_text, insert_text, suffix_text);

                        // The mutable borrow is fine now because we're assigning to the element
                        // in the vector, not the `current_line` reference.
                        current_lines[current_row] = new_line.clone();

                        let mut new_textarea = TextArea::new(current_lines);
                        set_textarea_delafult_style!(new_textarea);
                        let new_cursor_col =
                            prefix_text.chars().count() + insert_text.chars().count();
                        new_textarea.move_cursor(tui_textarea::CursorMove::Jump(
                            current_row as u16,
                            new_cursor_col as u16,
                        ));

                        self.textarea = new_textarea;
                    }
                }
            }
        }

        self.cancel_completion();
        self.mode = Mode::Insert;
        self.status = "Insert".to_string();
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
        match &self.search_state.search_type {
            SearchType::Backlinks => self.search_backlinks()?,
            SearchType::Tags => self.search_tags()?,
            SearchType::Files => self.search_files()?,
            // --- New Call ---
            SearchType::CustomSql => self.search_custom_sql()?,
            SearchType::CustomLua { provider, .. } => {
                let globals = self.lua.globals();
                if let Ok(func) = globals.get::<_, mlua::Function>(provider.as_str()) {
                    match func.call::<_, Vec<String>>(self.search_state.query.clone()) {
                        Ok(results) => {
                            // We map them to (String, None) since Lua handles the execution
                            self.search_state.results =
                                results.into_iter().map(|s| (s, None)).collect();
                        }
                        Err(e) => self.status = format!("Lua Search Error: {}", e),
                    }
                }
            }
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
        let (row, current_col) = self.textarea.cursor();
        let line = self.textarea.lines()[row].clone();
        let mut target = if let Some(wikilink) = self.extract_wikilink(&line, current_col) {
            wikilink
        } else {
            // Use file_name from files table for the current file
            let file_name: String = Path::new(&self.file_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_default()
                .to_string();
            file_name
        };

        if !target.ends_with(".md") {
            target = format!("{}.md", target).clone();
        };
        let query = "SELECT DISTINCT f.file_name, f.id
                     FROM backlinks b
                     JOIN files f ON b.file_id = f.id
                     JOIN files fp ON b.backlink_id = fp.id
                     WHERE fp.file_name LIKE ? AND f.id != ?";
        let mut stmt = self.db.prepare(query)?;
        let results = stmt
            .query_map(params![format!("{}", target), self.file_id], |row| {
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
        let query = "SELECT file_name, id, json_extract (files.metadata, '$.created_at') as created_at FROM files WHERE file_name LIKE ? ORDER BY created_at DESC";
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

    fn extract(&self, line: &str, cursor_col: usize, start: &str, end: &str) -> Option<String> {
        for (start_byte_index, _) in line.match_indices(start) {
            let mut content_start_byte = start_byte_index + start.len();
            // Skip whitespace after start delimiter
            while content_start_byte < line.len() && line[content_start_byte..].starts_with(' ') {
                content_start_byte += 1;
            }
            // Use end of line if end delimiter is empty, otherwise find end delimiter
            let end_byte_index = if end.is_empty() {
                line.len()
            } else {
                line[content_start_byte..]
                    .find(end)
                    .map(|relative| content_start_byte + relative)
                    .unwrap_or(line.len())
            };
            let start_char_index = line[..start_byte_index].chars().count();
            let end_char_index = line[..end_byte_index].chars().count();
            if cursor_col >= start_char_index && cursor_col <= end_char_index {
                if content_start_byte <= end_byte_index {
                    let content = line[content_start_byte..end_byte_index].trim();
                    if !content.is_empty() {
                        return Some(content.to_string());
                    }
                }
            }
        }
        None
    }

    fn extract_tag(&self, line: &str, cursor_col: usize) -> Option<String> {
        // Try normal tag (#tag)
        if let Some(result) = self.extract(line, cursor_col, "#", " ") {
            return Some(result);
        }
        // Fallback to YAML tag (- tag)
        self.extract(line, cursor_col, " - ", "")
    }

    fn extract_wikilink(&self, line: &str, cursor_col: usize) -> Option<String> {
        self.extract(line, cursor_col, "[[", "]]")
    }
    fn select_search_result(&mut self) -> Result<(), EditorError> {
        if let Some(selected) = self.search_state.list_state.selected() {
            if let Some((display_text, file_id)) = self.search_state.results.get(selected).cloned()
            {
                match &self.search_state.search_type {
                    SearchType::Backlinks | SearchType::Files | SearchType::CustomSql => {
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
                        } else if self.search_state.search_type == SearchType::CustomSql {
                            self.status =
                                "Custom SQL query result does not contain a file ID. Cannot open."
                                    .to_string();
                            return Ok(()); //Cancel search immediately
                        }
                    }
                    SearchType::Tags => {
                        self.load_tag_files(&display_text)?;
                        if !self.tag_files.is_empty() {
                            self.mode = Mode::TagFiles;
                        } else {
                            self.cancel_search();
                        }
                    }
                    SearchType::CustomLua { on_select, .. } => {
                        let globals = self.lua.globals();
                        if let Ok(func) = globals.get::<_, mlua::Function>(on_select.as_str()) {
                            if let Err(e) = func.call::<_, ()>(display_text.clone()) {
                                self.status = format!("Lua Select Error: {}", e);
                            }
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
            self.sort_asc = false;
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

    fn delete_file(&mut self, path: &str) -> Result<(), EditorError> {
        let full_path = Path::new(&self.base_dir)
            .join(path)
            .to_string_lossy()
            .to_string();
        let output = Command::new("markdown-scanner")
            .arg("--delete")
            .arg(&full_path)
            .arg(&self.base_dir)
            .output()?;
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(EditorError::Scanner(error_msg));
        }
        fs::remove_file(&full_path)?;
        self.remove_node(path);
        Ok(())
    }

    fn delete_selected_file(&mut self) -> Result<(), EditorError> {
        if let Some(selected) = self.tree_state.selected() {
            let item = self.visible_items[selected].clone();
            if !item.is_dir {
                self.delete_file(&item.path)?;
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
            self.delete_file(&path)?;
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
                let path = Path::new(&item.path)
                    .parent()
                    .unwrap_or(Path::new(""))
                    .to_string_lossy()
                    .to_string();
                if path == "" {
                    self.base_dir.clone()
                } else {
                    path
                }
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
            let output_delete = Command::new("markdown-scanner")
                .arg("--delete")
                .arg(&old_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_delete.status.success() {
                let error_msg = String::from_utf8_lossy(&output_delete.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }
            fs::rename(&old_full, &new_full)?;
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
            let output_delete = Command::new("markdown-scanner")
                .arg("--delete")
                .arg(&old_full)
                .arg(&self.base_dir)
                .output()?;
            if !output_delete.status.success() {
                let error_msg = String::from_utf8_lossy(&output_delete.stderr).into_owned();
                return Err(EditorError::Scanner(error_msg));
            }

            fs::rename(&old_full, &new_full)?;

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
                // 1. Convert the raw key event to a string (e.g., "j", "<C-s>", "<Esc>")
                let key_str = Self::event_to_string(event);

                // 2. Add it to our running sequence
                self.key_sequence.push_str(&key_str);
                let sequence = self.key_sequence.clone();

                // 3. Check Lua keymaps
                let (has_exact, has_partial) = {
                    let maps = self.normal_keymaps.borrow();
                    let exact = maps.contains_key(&sequence);
                    // Check if any mapped key starts with what we typed (e.g. typed "g", map has "gg")
                    let partial = maps
                        .keys()
                        .any(|k| k.starts_with(&sequence) && k != &sequence);
                    (exact, partial)
                };

                if has_exact {
                    // --- 1. SYNC STATE TO LUA BEFORE EXECUTION ---
                    {
                        let mut ctx = self.shared_context.borrow_mut();
                        ctx.lines = self.textarea.lines().to_vec();
                        ctx.cursor_row = self.textarea.cursor().0;
                        ctx.cursor_col = self.textarea.cursor().1;
                        ctx.current_file = self.file_path.clone();
                    }
                    // 2. Execute the Lua function
                    let mut lua_error = None;
                    {
                        let maps = self.normal_keymaps.borrow();
                        let reg_key = maps.get(&sequence).unwrap();
                        let func: mlua::Function = self.lua.registry_value(reg_key).unwrap();

                        if let Err(e) = func.call::<_, ()>(()) {
                            lua_error = Some(e.to_string());
                        }
                    }

                    if let Some(err) = lua_error {
                        self.status = format!("Lua execution error: {}", err);
                    }

                    self.key_sequence.clear();
                    self.execute_lua_commands()?;
                } else if has_partial {
                    // Valid start of a sequence, wait for the next keypress
                    self.status = format!("Waiting: {}", sequence);
                } else {
                    // Invalid sequence or unmapped key, clear to start fresh next press
                    self.key_sequence.clear();
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

                        // This ensures correct string slicing with multi-byte characters.
                        let col_bytes = line
                            .char_indices()
                            .nth(col)
                            .map(|(i, _)| i)
                            .unwrap_or(line.len());

                        let check_completion =
                            |line_ref: &str, cursor_col_bytes: usize| -> Option<CompletionType> {
                                if let Some(s) = line_ref.get(..cursor_col_bytes) {
                                    if s.ends_with("[[") {
                                        return Some(CompletionType::File);
                                    } else if s.ends_with('#') {
                                        return Some(CompletionType::Tag);
                                    } else if s.ends_with('@') {
                                        return Some(CompletionType::Variable);
                                    }
                                }
                                None
                            };

                        if let Some(comp_type) = check_completion(&line, col_bytes) {
                            self.start_completion(comp_type);
                            self.update_completion()?;
                        }
                    }
                    ratatui::crossterm::event::KeyCode::Backspace => {
                        self.textarea.input(input);
                        let (row, col) = self.textarea.cursor();
                        let line = self.textarea.lines()[row].clone();

                        let col_bytes = line.chars().take(col).collect::<String>().len();

                        // If the trigger character is deleted, cancel completion.
                        if self.completion_state.active {
                            let mut should_cancel = false;
                            match self.completion_state.completion_type {
                                CompletionType::File => {
                                    if !line.get(..col_bytes).map_or(false, |s| s.contains("[[")) {
                                        should_cancel = true;
                                    }
                                }
                                CompletionType::Tag => {
                                    if !line.get(..col_bytes).map_or(false, |s| s.contains("#")) {
                                        should_cancel = true;
                                    }
                                }
                                _ => {}
                            }
                            if should_cancel {
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
                    } else if self.command.starts_with("echo ") {
                        let message = self.command.trim_start_matches("echo ").to_string();
                        self.echo(&message)?;
                    } else if self.command.starts_with("delete") {
                        // tree view is good but this also need.
                        let delete_path = self.file_path.clone();
                        self.delete_file(&delete_path)?;
                        self.textarea.clear_mask_char();
                        self.open_wikilink_file("index.md".to_string())?; //TODO open something else in the future
                        self.status = format!("Deleted file: {}", delete_path);
                    } else if self.command.starts_with("lua ") {
                        let code = self.command.trim_start_matches("lua ").to_string();
                        // Execute the code directly
                        match self.lua.load(&code).exec() {
                            Ok(_) => self.status = "Lua executed".to_string(),
                            Err(e) => self.status = format!("Lua err: {}", e),
                        }
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
                    if self.search_state.search_type == SearchType::CustomSql
                        && self.search_state.results.is_empty()
                    {
                        // Execute the custom query
                        self.update_search_results()?;
                        // The next input will either be Up/Down or a second Enter.
                    } else if !self.search_state.results.is_empty() {
                        // 2. Second Enter: Select the file from the displayed list
                        self.select_search_result()?;
                        self.execute_lua_commands()?;
                    }
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
                    // Standard search types (Tags, Files, Backlinks) update instantly
                    if self.search_state.search_type != SearchType::CustomSql {
                        self.update_search_results()?;
                    }
                }
                ratatui::crossterm::event::KeyCode::Backspace => {
                    self.search_state.query.pop();
                    // Clear the previous results if the query is being modified
                    if self.search_state.search_type == SearchType::CustomSql {
                        self.search_state.results.clear();
                        self.search_state.list_state.select(None);
                    }
                    if self.search_state.search_type != SearchType::CustomSql {
                        self.update_search_results()?;
                    }
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
                let key_str = Self::event_to_string(event);
                self.key_sequence.push_str(&key_str);
                let sequence = self.key_sequence.clone();

                let (has_exact, has_partial) = {
                    let maps = self.visual_keymaps.borrow(); // NOTE: visual_keymaps here!
                    let exact = maps.contains_key(&sequence);
                    let partial = maps
                        .keys()
                        .any(|k| k.starts_with(&sequence) && k != &sequence);
                    (exact, partial)
                };

                if has_exact {
                    // Sync State
                    {
                        let mut ctx = self.shared_context.borrow_mut();
                        ctx.lines = self.textarea.lines().to_vec();
                        ctx.cursor_row = self.textarea.cursor().0;
                        ctx.cursor_col = self.textarea.cursor().1;
                        ctx.current_file = self.file_path.clone();
                        ctx.visual_anchor = self.visual_anchor; // Sync the anchor
                    }

                    let mut lua_error = None;
                    {
                        let maps = self.visual_keymaps.borrow();
                        let reg_key = maps.get(&sequence).unwrap();
                        let func: mlua::Function = self.lua.registry_value(reg_key).unwrap();
                        if let Err(e) = func.call::<_, ()>(()) {
                            lua_error = Some(e.to_string());
                        }
                    }

                    if let Some(err) = lua_error {
                        self.status = format!("Lua error: {}", err);
                    }
                    self.key_sequence.clear();
                    self.execute_lua_commands()?;

                    return Ok(()); // Stop here, Lua handled it!
                } else if has_partial {
                    self.status = format!("Waiting: {}", sequence);
                    return Ok(()); // Stop here, waiting for next key
                } else {
                    self.key_sequence.clear();
                }

                // --- EXISTING HARDCODED RUST LOGIC CONTINUES BELOW ---
                let mut input = tui_textarea::Input::from(event);
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
                                let start_row = range.0.0;
                                let start_col = range.0.1;
                                let end_row = range.1.0;
                                let end_col = range.1.1;
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
                                let start_row = range.0.0;
                                let start_col = range.0.1;
                                let end_row = range.1.0;
                                let end_col = range.1.1;
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
                                self.mode = Mode::Normal;
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
                // If CustomSql is active, we don't display results until Enter is hit.
                if self.search_state.search_type == SearchType::CustomSql
                    && self.search_state.results.is_empty()
                    && !self.search_state.query.is_empty()
                {
                    // If typing a custom query, show the editor content in chunks[0]
                    self.render_editor(f, chunks[0])?;
                    // The actual query input is handled below in chunks[2]
                } else {
                    // Render search results for all modes (Backlinks, Tags, Files, and results of CustomSql)
                    let title = match &self.search_state.search_type {
                        // Add & here as well just to be safe
                        SearchType::Backlinks => format!("Backlinks: {}", self.search_state.query),
                        SearchType::Tags => format!("Tags: {}", self.search_state.query),
                        SearchType::Files => format!("Files: {}", self.search_state.query),
                        SearchType::CustomSql => format!(
                            "Custom SQL Results (Press Enter to Open): {}",
                            self.search_state.query
                        ),
                        SearchType::CustomLua { .. } => {
                            format!("Lua Search: {}", self.search_state.query)
                        } // <--- ADD THIS LINE
                        SearchType::None => "Search".to_string(),
                    };

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

        let command_text = match self.mode {
            Mode::Command => format!(":{}", self.command),
            Mode::Search => {
                if self.search_state.search_type == SearchType::CustomSql {
                    format!("SQL: {}", self.search_state.query)
                } else {
                    format!("/{}", self.search_state.query)
                }
            }
            Mode::Normal | Mode::FileTree if !self.key_sequence.is_empty() => {
                format!("{}", self.key_sequence)
            }
            _ => String::new(),
        };

        let command = Paragraph::new(command_text).style(Style::default().fg(Color::White));
        f.render_widget(command, chunks[2]);

        Ok(())
    }

    fn render_editor(&mut self, f: &mut Frame, area: Rect) -> Result<(), EditorError> {
        // 1. Check if the cursor moved to an image
        self.check_image_at_cursor()?;

        // 2. Draw outer borders and get the safe inner drawing area
        let inner_area = self.render_borders(f, area);

        self.editor_width = inner_area.width;
        // 3. Update scroll positions based on the cursor and inner area
        self.update_scrolling(inner_area);

        // 4. Process text (highlighting, virtual text) and render the paragraph/cursor
        self.render_text_and_cursor(f, inner_area)?;

        // 5. Render the image overlay if one is active
        self.render_image_overlay(f, area, inner_area);

        // 6. Render the completion popup if active
        self.render_completion_popup(f, inner_area);

        Ok(())
    }

    // --- 1. Cursor check for images ---
    fn check_image_at_cursor(&mut self) -> Result<(), EditorError> {
        let (cursor_row, cursor_col) = self.textarea.cursor();
        let (last_cursor_row, last_cursor_col) = self.completion_state.trigger_start;

        if cursor_row != last_cursor_row || cursor_col != last_cursor_col {
            self.load_image_at_cursor()?;
            self.completion_state.trigger_start = (cursor_row, cursor_col);
        }
        Ok(())
    }

    // --- 2. Draw Borders and Calculate Safe Inner Area ---
    fn render_borders(&self, f: &mut Frame, area: Rect) -> Rect {
        let mode_title = if self.read_mode {
            "[Read Mode - Wrapped]"
        } else {
            "[Edit Mode]"
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!("Midetor {}", mode_title))
            .style(Style::default().fg(Color::White));

        let inner_area = block.inner(area);

        if !self.image_full_screen {
            f.render_widget(block, area);
        }

        inner_area
    }

    // --- 3. Adjust Scrolling ---
    fn update_scrolling(&mut self, inner_area: Rect) {
        let area_height = inner_area.height as usize;
        let area_w = inner_area.width.max(1) as usize;
        let total_lines = self.textarea.lines().len();
        let cursor_row = self.textarea.cursor().0;
        let cursor_col = self.textarea.cursor().1;

        if self.read_mode {
            let mut absolute_cursor_y = 0;
            for r in 0..=cursor_row {
                // Fetch the expanded line string and the newly mapped cursor index
                let (expanded_line, mapped_col) = self.get_expanded_line_info(r, cursor_col);

                if r == cursor_row {
                    let (y, _) =
                        Self::calculate_cursor_position(&expanded_line, mapped_col, area_w);
                    absolute_cursor_y += y;
                } else {
                    absolute_cursor_y += Self::calculate_visual_lines(&expanded_line, area_w);
                }
            }

            if absolute_cursor_y < self.visual_scroll_y as usize {
                self.visual_scroll_y = absolute_cursor_y as u16;
            } else if absolute_cursor_y >= (self.visual_scroll_y as usize + area_height) {
                self.visual_scroll_y = (absolute_cursor_y - area_height + 1) as u16;
            }
        } else {
            let visible_lines_count = area_height.min(total_lines);
            if cursor_row < self.scroll_offset {
                self.scroll_offset = cursor_row;
            } else if cursor_row >= self.scroll_offset + visible_lines_count {
                self.scroll_offset =
                    cursor_row.saturating_sub(visible_lines_count.saturating_sub(1));
            }

            let max_scroll = total_lines.saturating_sub(visible_lines_count);
            self.scroll_offset = self.scroll_offset.min(max_scroll);

            if cursor_col < self.horizontal_scroll_offset {
                self.horizontal_scroll_offset = cursor_col;
            } else if cursor_col >= self.horizontal_scroll_offset + area_w {
                self.horizontal_scroll_offset = cursor_col.saturating_sub(area_w.saturating_sub(1));
            }
        }
    }

    // --- 4 & 5. Process Text & Render Paragraph/Cursor ---
    fn render_text_and_cursor(
        &mut self,
        f: &mut Frame,
        inner_area: Rect,
    ) -> Result<(), EditorError> {
        let area_height = inner_area.height as usize;
        let area_width = inner_area.width.max(1) as usize;
        let total_lines = self.textarea.lines().len();

        // Calculate the exact starting chunk based on visual scroll
        let mut start_logical_row = self.scroll_offset;
        let mut scroll_remainder = 0;

        if self.read_mode {
            let mut visual_y = 0;
            for r in 0..total_lines {
                let (expanded_line, _) = self.get_expanded_line_info(r, 0);
                let lines_for_this_row = Self::calculate_visual_lines(&expanded_line, area_width);

                if visual_y + lines_for_this_row > self.visual_scroll_y as usize {
                    start_logical_row = r;
                    scroll_remainder = self.visual_scroll_y as usize - visual_y;
                    break;
                }
                visual_y += lines_for_this_row;
            }
        }

        // Give Read Mode a generous buffer of lines so it doesn't cut off at the bottom of the screen
        let visible_lines_count = if self.read_mode {
            area_height
        } else {
            area_height.min(total_lines)
        };
        let end_line =
            (start_logical_row + visible_lines_count + (if self.read_mode { 15 } else { 0 }))
                .min(total_lines);

        let syntax = self
            .syntax_set
            .find_syntax_by_extension("md")
            .unwrap_or_else(|| self.syntax_set.find_syntax_by_name("Markdown").unwrap());

        let mut highlighter = HighlightLines::new(syntax, &self.theme);
        let mut visible_text = Vec::new();
        let selection_range = self.textarea.selection_range();

        for logical_row in start_logical_row..end_line {
            let line = &self.textarea.lines()[logical_row];
            let trimmed_line = line.trim();

            let mut is_vid_header = false;
            if trimmed_line.starts_with("```vid") {
                is_vid_header = true;
            }

            let line_with_nl = format!("{}\n", line);
            let ranges = highlighter
                .highlight_line(&line_with_nl, &self.syntax_set)
                .map_err(|e| EditorError::SyntaxHighlighting(e.to_string()))?;

            let mut new_spans = Vec::new();
            let mut col_tracker = 0;

            // --- Fetch and sort virtual text for this specific line ---
            let mut line_v_texts = self
                .virtual_texts
                .get(&logical_row)
                .cloned()
                .unwrap_or_default();
            line_v_texts.sort_by_key(|v| v.0); // Sort by column index
            let mut v_idx = 0;

            for (style, text_segment) in ranges {
                let text = text_segment.trim_end_matches('\n');
                if text.is_empty() {
                    continue;
                }

                let color = Color::Rgb(style.foreground.r, style.foreground.g, style.foreground.b);
                let span_style = Style::default().fg(color);

                // Slice token dynamically to insert virtual text inside
                let text_chars: Vec<char> = text.chars().collect();
                let mut char_idx = 0;

                while char_idx < text_chars.len() {
                    // 1. Inject virtual text at exactly this column position
                    while v_idx < line_v_texts.len() && line_v_texts[v_idx].0 == col_tracker {
                        let (_, ref v_text, ref v_color) = line_v_texts[v_idx];
                        let parsed_color = match v_color.to_lowercase().as_str() {
                            "red" => Color::Red,
                            "green" => Color::Green,
                            "blue" => Color::LightBlue,
                            "yellow" => Color::Yellow,
                            "gray" => Color::DarkGray,
                            _ => Color::Gray,
                        };
                        let v_style = Style::default()
                            .fg(parsed_color)
                            .add_modifier(Modifier::ITALIC);
                        new_spans.push(Span::styled(v_text.clone(), v_style));
                        v_idx += 1;
                    }

                    // 2. Find boundary to the next virtual text (or end of segment)
                    let mut next_boundary = text_chars.len();
                    if v_idx < line_v_texts.len() && line_v_texts[v_idx].0 > col_tracker {
                        let dist = line_v_texts[v_idx].0 - col_tracker;
                        if char_idx + dist < next_boundary {
                            next_boundary = char_idx + dist;
                        }
                    }

                    let sub_text: String = text_chars[char_idx..next_boundary].iter().collect();
                    let sub_len = sub_text.chars().count();

                    let mut current_span_style = span_style;
                    if let Some(((start_r, start_c), (end_r, end_c))) = selection_range {
                        if (logical_row > start_r
                            || (logical_row == start_r && col_tracker >= start_c))
                            && (logical_row < end_r
                                || (logical_row == end_r && col_tracker < end_c))
                        {
                            current_span_style = current_span_style.bg(Color::LightBlue);
                        }
                    }

                    if !self.read_mode {
                        let span_end = col_tracker + sub_len;
                        if span_end > self.horizontal_scroll_offset {
                            let start_char = if col_tracker < self.horizontal_scroll_offset {
                                self.horizontal_scroll_offset - col_tracker
                            } else {
                                0
                            };
                            let sliced_text: String = sub_text.chars().skip(start_char).collect();
                            if !sliced_text.is_empty() {
                                new_spans.push(Span::styled(sliced_text, current_span_style));
                            }
                        }
                    } else {
                        new_spans.push(Span::styled(sub_text, current_span_style));
                    }

                    char_idx = next_boundary;
                    col_tracker += sub_len;
                }
            }

            // --- Catch any trailing End-Of-Line (EOL) virtual text ---
            while v_idx < line_v_texts.len() {
                let (_, ref v_text, ref v_color) = line_v_texts[v_idx];
                let parsed_color = match v_color.to_lowercase().as_str() {
                    "red" => Color::Red,
                    "green" => Color::Green,
                    "blue" => Color::LightBlue,
                    "yellow" => Color::Yellow,
                    "gray" => Color::DarkGray,
                    _ => Color::Gray,
                };
                new_spans.push(Span::styled(
                    format!(" {}", v_text),
                    Style::default()
                        .fg(parsed_color)
                        .add_modifier(Modifier::ITALIC),
                ));
                v_idx += 1;
            }

            if is_vid_header {
                new_spans = Self::build_vid_virtual_text(
                    self.textarea.lines(),
                    logical_row,
                    &self.yt_videos,
                    self.horizontal_scroll_offset,
                    self.read_mode,
                );
            }

            visible_text.push(Line::from(new_spans));
        }

        if !self.image_full_screen {
            let mut paragraph = Paragraph::new(visible_text).style(self.textarea.style());

            if self.read_mode {
                // Apply Ratatui's native intra-line scrolling
                paragraph = paragraph
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .scroll((scroll_remainder as u16, 0));
            }

            f.render_widget(paragraph, inner_area);

            let (cursor_row, cursor_col) = self.textarea.cursor();

            if !self.read_mode {
                if cursor_row >= self.scroll_offset
                    && cursor_row < self.scroll_offset + visible_lines_count
                {
                    let screen_row = (cursor_row - self.scroll_offset) as u16;
                    let screen_col =
                        (cursor_col.saturating_sub(self.horizontal_scroll_offset)) as u16;
                    let cursor_x = screen_col.min(inner_area.width.saturating_sub(1));

                    let cursor_area = Rect {
                        x: inner_area.x + cursor_x,
                        y: inner_area.y + screen_row,
                        width: 1,
                        height: 1,
                    };

                    let line = self
                        .textarea
                        .lines()
                        .get(cursor_row)
                        .cloned()
                        .unwrap_or_default();
                    let ch: char = line.chars().nth(cursor_col).unwrap_or(' ');
                    let cursor_style = Style::default().bg(Color::White).fg(Color::Black);
                    let cursor_span = Span::styled(ch.to_string(), cursor_style);

                    f.render_widget(Paragraph::new(cursor_span), cursor_area);
                }
            } else {
                // --- WRAPPED MODE ABSOLUTE CURSOR ---
                let mut absolute_cursor_y = 0;
                let mut cursor_screen_x = 0;

                for r in 0..=cursor_row {
                    let (expanded_line, mapped_col) = self.get_expanded_line_info(r, cursor_col);
                    if r == cursor_row {
                        // THIS FUNCTION CALCULATES THE TRUE WORD-WRAPPED X and Y
                        let (y, x) =
                            Self::calculate_cursor_position(&expanded_line, mapped_col, area_width);
                        absolute_cursor_y += y;
                        cursor_screen_x = x;
                    } else {
                        absolute_cursor_y +=
                            Self::calculate_visual_lines(&expanded_line, area_width);
                    }
                }

                // If the cursor is currently inside the visual viewport, draw it
                if absolute_cursor_y >= self.visual_scroll_y as usize
                    && absolute_cursor_y < (self.visual_scroll_y as usize + area_height)
                {
                    let screen_y = (absolute_cursor_y - self.visual_scroll_y as usize) as u16;

                    // FIXED: Use the simulated X position
                    let screen_x = cursor_screen_x as u16;

                    let cursor_area = Rect {
                        x: inner_area.x + screen_x,
                        y: inner_area.y + screen_y,
                        width: 1,
                        height: 1,
                    };

                    let line = self
                        .textarea
                        .lines()
                        .get(cursor_row)
                        .cloned()
                        .unwrap_or_default();
                    let ch: char = line.chars().nth(cursor_col).unwrap_or(' ');
                    let cursor_span = Span::styled(
                        ch.to_string(),
                        Style::default().bg(Color::White).fg(Color::Black),
                    );
                    f.render_widget(Paragraph::new(cursor_span), cursor_area);
                }
            }
        }

        Ok(())
    }

    fn build_vid_virtual_text<'a>(
        lines: &[String],
        logical_row: usize,
        yt_videos: &HashMap<String, String>,
        horizontal_scroll_offset: usize,
        read_mode: bool,
    ) -> Vec<Span<'a>> {
        let mut title = "Title Not Found";

        // Peek at the next line to find the URL
        if let Some(next_line) = lines.get(logical_row + 1) {
            let url = next_line.trim();
            if !url.is_empty() {
                for (db_url, db_title) in yt_videos {
                    if url.contains(db_url) || db_url.contains(url) {
                        title = db_title.as_str();
                        break;
                    }
                }
            }
        }

        let combined_text = format!("```vid  ➔ {}", title);
        let mut final_text = combined_text.clone();

        // Apply horizontal scrolling if not in read mode
        if !read_mode && horizontal_scroll_offset > 0 {
            let text_chars: Vec<char> = combined_text.chars().collect();
            final_text = text_chars
                .into_iter()
                .skip(horizontal_scroll_offset)
                .collect();
        }

        vec![Span::styled(
            final_text,
            Style::default()
                .fg(Color::LightCyan)
                .add_modifier(Modifier::BOLD),
        )]
    }

    // --- 6. Render Image Overlay ---
    fn render_image_overlay(&mut self, f: &mut Frame, area: Rect, inner_area: Rect) {
        let area_height = inner_area.height as usize;
        let visible_lines_count = area_height.min(self.textarea.lines().len());

        if let (Some(picker), Some(dyn_img), Some(image_row)) = (
            self.image_picker.as_mut(),
            self.current_image.as_ref(),
            self.current_image_line,
        ) {
            let (image_area, title_str) = if self.image_full_screen {
                (area, "Image (Full Screen)")
            } else {
                let popup_width = (inner_area.width as f32 * 0.4)
                    .max(30.0)
                    .min(inner_area.width as f32) as u16;
                let popup_height = (inner_area.height as f32 * 0.6)
                    .max(10.0)
                    .min(inner_area.height as f32) as u16;
                let max_y = inner_area.y + inner_area.height.saturating_sub(popup_height);
                let max_x = inner_area.x + inner_area.width.saturating_sub(popup_width);

                let mut popup_y;
                if self.read_mode
                    || image_row < self.scroll_offset
                    || image_row >= self.scroll_offset + visible_lines_count
                {
                    popup_y = inner_area.y;
                } else {
                    let screen_row = (image_row - self.scroll_offset) as u16;
                    popup_y = inner_area.y + screen_row;
                }
                popup_y = popup_y.min(max_y);

                let popup_area = Rect {
                    x: max_x,
                    y: popup_y,
                    width: popup_width,
                    height: popup_height,
                };

                (popup_area, "Image")
            };

            if self.image_protocol.is_none() || self.last_image_area != Some(image_area) {
                self.image_protocol = Some(picker.new_resize_protocol(dyn_img.clone()));
                self.last_image_area = Some(image_area);
            }

            if let Some(image_protocol) = self.image_protocol.as_mut() {
                let img_block = Block::default().borders(Borders::ALL).title(title_str);
                let image_widget = ratatui_image::StatefulImage::default()
                    .resize(Resize::Scale(Some(FilterType::Triangle)));

                f.render_widget(ratatui::widgets::Clear, image_area);
                f.render_widget(img_block, image_area);

                let margin = Margin::new(1, 1);
                f.render_stateful_widget(image_widget, image_area.inner(margin), image_protocol);

                if let Some(Err(e)) = image_protocol.last_encoding_result() {
                    self.status = format!("Image encoding error: {}", e);
                }
            }
        }
    }

    // --- 7. Render Completion Popup ---
    fn render_completion_popup(&mut self, f: &mut Frame, inner_area: Rect) {
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
                            CompletionType::Variable => "Variable",
                            CompletionType::None => "",
                        })
                        .style(Style::default().fg(Color::White).bg(Color::Black)),
                )
                .highlight_style(Style::default().bg(Color::White).fg(Color::Black));

            let popup_width = 40.min(inner_area.width);
            let popup_height = ((self.completion_state.suggestions.len().min(5) + 2) as u16)
                .min(inner_area.height);
            let popup_area = Rect {
                x: inner_area.x + inner_area.width.saturating_sub(popup_width),
                y: inner_area.y,
                width: popup_width,
                height: popup_height,
            };

            f.render_widget(ratatui::widgets::Clear, popup_area);
            f.render_stateful_widget(list, popup_area, &mut self.completion_state.list_state);
        }
    }

    fn process_template_command(&mut self, template_path_str: &str) -> Result<(), EditorError> {
        // 1. Define the template path. This should be a configurable path.
        let template_path = Path::new(&self.base_dir).join(template_path_str);

        // 2. Read the template file.
        let template_content = fs::read_to_string(&template_path).map_err(|e| {
            EditorError::Io(std::io::Error::new(
                e.kind(),
                format!("Could not open template file: {}", e),
            ))
        })?;

        // 3. Get current date, time, and title.
        let now = Local::now();
        let current_date = now.format("%Y-%m-%d").to_string();
        let current_time = now.format("%H:%M:%S").to_string();
        let title = gettitle!(&self.file_path);
        // Path::new(&self.file_path)
        // .file_stem()
        // .and_then(|s| s.to_str())
        // .unwrap_or_default()
        // .to_string();

        // 4. Replace template variables.
        let processed_content = template_content
            .replace("{{date}}", &current_date)
            .replace("{{time}}", &current_time)
            .replace("{{title}}", &title);

        // 5. Get current buffer content.
        let current_content = self.textarea.lines().join("\n");

        // 6. Combine template and current content.
        let new_content = format!("{}\n{}", processed_content, current_content);
        let new_lines: Vec<String> = new_content.lines().map(|s| s.to_string()).collect();

        // 7. Update the textarea.
        let mut new_textarea = TextArea::new(new_lines);
        set_textarea_delafult_style!(new_textarea);
        self.textarea = new_textarea;

        self.status = "Template processed and inserted.".to_string();
        Ok(())
    }

    fn extract_image_paths(&self) -> Vec<(String, usize)> {
        let mut image_paths = Vec::new();
        let lines = self.textarea.lines();
        let re = regex::Regex::new(r"\[\[(.*?)\.(jpg|jpeg|avif|png|webp)\]\]").unwrap();
        for (row, line) in lines.iter().enumerate() {
            for cap in re.captures_iter(line) {
                if let Some(filename) = cap.get(1) {
                    let full_filename = format!("{}.{}", filename.as_str(), cap[2].to_string());
                    image_paths.push((full_filename, row));
                }
            }
        }
        image_paths
    }

    fn resolve_image_path(&self, image_path: &str) -> Result<String, EditorError> {
        // println!("Resolving image path for: {}", image_path);

        let query = "SELECT path FROM files WHERE file_name = ? OR path = ?";
        let mut stmt = self.db.prepare(query)?;
        let path_result = stmt.query_row(params![image_path, image_path], |row| {
            row.get::<_, String>(0)
        });

        match path_result {
            Ok(path) => {
                // println!("Found in database: {}", path);
                let full_path = Path::new(&path)
                    .canonicalize()
                    .map_err(|e| {
                        EditorError::FileNotFound(format!("Invalid path {}: {}", path, e))
                    })?
                    .to_string_lossy()
                    .to_string();
                // println!("Canonicalized path: {}", full_path);
                Ok(full_path)
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let full_path = Path::new(&self.base_dir)
                    .join(image_path)
                    .canonicalize()
                    .map_err(|e| {
                        EditorError::FileNotFound(format!("Invalid path {}: {}", image_path, e))
                    })?
                    .to_string_lossy()
                    .to_string();
                // println!("Fallback path: {}", full_path);
                Ok(full_path)
            }
            Err(e) => Err(EditorError::Scanner(format!("Database error: {}", e))),
        }
    }

    fn load_image_at_cursor(&mut self) -> Result<(), EditorError> {
        let cursor_row = self.textarea.cursor().0;
        let cursor_col = self.textarea.cursor().1;
        let current_line = self
            .textarea
            .lines()
            .get(cursor_row)
            .cloned()
            .unwrap_or_default();

        // Check current line for wikilink
        let wikilink = self.extract_wikilink(&current_line, cursor_col);
        let is_image = wikilink.as_ref().map_or(false, |link| {
            link.to_lowercase().ends_with(".jpg")
                || link.to_lowercase().ends_with(".jpeg")
                || link.to_lowercase().ends_with(".avif")
                || link.to_lowercase().ends_with(".png")
                || link.to_lowercase().ends_with(".webp")
        });

        // Only load if wikilink is a valid image and different from last
        if is_image && wikilink != self.last_wikilink {
            self.last_wikilink = wikilink.clone();
            if let Some(image_path) = wikilink {
                match self.resolve_image_path(&image_path) {
                    Ok(full_path) => match image::ImageReader::open(&full_path) {
                        Ok(reader) => match reader.decode() {
                            Ok(dyn_img) => {
                                self.current_image = Some(dyn_img);
                                self.current_image_line = Some(cursor_row);

                                self.image_protocol = None;
                                self.last_image_area = None;

                                self.current_image_index = self
                                    .image_paths
                                    .iter()
                                    .position(|(path, _)| path == &image_path)
                                    .unwrap_or(0);
                                self.status = format!("Loaded image: {}", image_path);
                            }
                            Err(e) => {
                                self.clear_image_state();
                                self.status = format!("Failed to load image {}: {}", image_path, e);
                            }
                        },
                        Err(e) => {
                            self.clear_image_state();
                            self.status = format!("No valid image at {}: {}", image_path, e);
                        }
                    },
                    Err(e) => {
                        self.clear_image_state();
                        self.status = format!("Failed to resolve image path {}: {}", image_path, e);
                    }
                }
            }
        } else if !is_image && self.current_image.is_some() {
            // Clear image if no valid image wikilink at cursor
            self.clear_image_state();
            self.last_wikilink = None;
        }

        Ok(())
    }
    fn clear_image_state(&mut self) {
        self.image_protocol = None;
        self.current_image = None;
        self.current_image_line = None;
        self.current_image_index = 0;
        self.last_image_area = None;
        self.image_full_screen = false;
    }

    fn search_custom_sql(&mut self) -> Result<(), EditorError> {
        let query = &self.search_state.query;

        if !query.trim().to_lowercase().starts_with("select") {
            self.status = "Only SELECT queries are allowed.".to_string();
            self.search_state.results = Vec::new();
            return Ok(());
        }

        let mut stmt = match self.db.prepare(query) {
            Ok(s) => s,
            Err(e) => {
                self.status = format!("SQL Prepare Error: {}", e);
                self.search_state.results = Vec::new();
                return Ok(());
            }
        };

        let mut results = Vec::new();

        let rows = stmt.query_map([], |row| {
            // Column 0: Display Text (Required)
            let display_text: String = row.get(0)?;

            // Column 1: File ID (Optional and needs careful handling)
            // We attempt to get the i64 value directly.
            // If it returns a Rusqlite error (like TypeMismatch or Null),
            // we map it to None, assuming the column exists but is empty or wrong type.
            // We use an explicit lifetime on the row reference: `row.get::<_, T>(idx)`

            let file_id: Option<i64> = match row.get(1) {
                Ok(val) => Some(val),
                // Catching all errors here is safe because if the row exists, any error
                // on the optional column just means we can't open a file for this result.
                // Specifically, rusqlite often returns `InvalidColumnIndex` if the column
                // isn't present in the query result set, or a conversion error if it's NULL/wrong type.
                Err(_) => None,
            };

            Ok((display_text, file_id))
        });

        match rows {
            Ok(mapped_rows) => {
                results = mapped_rows
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|e| EditorError::Database(e))?;
                self.status = format!(
                    "Custom SQL query executed. {} results found.",
                    results.len()
                );
            }
            Err(e) => {
                self.status = format!("SQL Query Execution Error: {}", e);
            }
        }

        self.search_state.results = results;

        Ok(())
    }

    fn playaudio(&mut self, index: usize, music_path: &str) -> Result<(), EditorError> {
        // --- 1. Extract the Wikilink (Filename) ---
        let (current_row, current_col) = self.textarea.cursor();
        let line = self.textarea.lines()[current_row].clone();

        let filename = if index < self.backlinks.len() {
            self.backlinks[index].0.clone()
        } else {
            self.extract_wikilink(&line, current_col).ok_or_else(|| {
                EditorError::InvalidBacklink("No valid wikilink found at cursor".to_string())
            })?
        };

        // --- 2. Clean Autocompletions (Your existing logic) ---
        if line.contains("[[") && !line.contains("]]") {
            let mut new_lines = self.textarea.lines().to_vec();
            new_lines[current_row] = line[..line.rfind("[[").unwrap_or(line.len())].to_string();
            self.textarea = tui_textarea::TextArea::new(new_lines);
            // set_textarea_delafult_style!(self.textarea);
            self.textarea
                .move_cursor(tui_textarea::CursorMove::Jump(current_row as u16, 0));
        }

        // --- 3. Construct and Verify Path ---
        // We join the constant MUSIC_DIR with the filename from the wikilink.
        let mut file_path = Path::new(music_path).join(&filename);

        if !file_path.exists() && !file_path.with_extension("mp3").exists() {
            // We'll look for either the exact filename or filename.mp3
            let target_name = filename.clone();
            let target_mp3 = format!("{}.mp3", filename);

            let found_path = WalkDir::new(music_path)
                .into_iter()
                .filter_map(|e| e.ok())
                .find(|entry| {
                    let name = entry.file_name().to_string_lossy();
                    name == target_name || name == target_mp3
                });

            if let Some(entry) = found_path {
                file_path = entry.path().to_path_buf();
            } else {
                return Err(EditorError::InvalidBacklink(format!(
                    "File '{}' not found in {}",
                    filename, music_path
                )));
            }
        } else if !file_path.exists() {
            // This handles the case where it WAS found in the root, but needed the .mp3 extension
            file_path = file_path.with_extension("mp3");
        }

        // --- 4. Stop Previous Audio & Start New ---
        self.stop_audio();

        let child_result = Command::new("vlc")
            .arg("-I")
            .arg("dummy")
            .arg("--play-and-exit")
            .arg(&file_path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();

        match child_result {
            Ok(child) => {
                self.audio_child = Some(child);
                self.status = format!("Playing: {}", filename);
            }
            Err(e) => {
                return Err(EditorError::InvalidBacklink(format!(
                    "Failed to start VLC: {}",
                    e
                )));
            }
        }

        Ok(())
    }

    pub fn stop_audio(&mut self) {
        // .take() moves the value out of the Option, leaving None behind.
        if let Some(mut child) = self.audio_child.take() {
            // Attempt to kill the process
            if let Err(e) = child.kill() {
                eprintln!("Failed to kill audio process: {}", e);
            }
            // Wait on the child to prevent "zombie" processes
            let _ = child.wait();
        }
        self.status = String::from("Audio stopped");
    }

    fn visual_move_down(&mut self) {
        let (row, col) = self.textarea.cursor();
        let width = self.editor_width.max(1) as usize;
        let line = self.textarea.lines().get(row).cloned().unwrap_or_default();
        let char_count = line.chars().count();

        // If dropping down keeps us on the same logical line (just a wrapped chunk)
        if col + width <= char_count {
            self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                row as u16,
                (col + width) as u16,
            ));
        } else {
            // We are on the last visual chunk of this line, jump to the next logical line
            let next_row = row + 1;
            if next_row < self.textarea.lines().len() {
                let visual_x = col % width; // Try to maintain the same horizontal X position
                let next_line_chars = self.textarea.lines()[next_row].chars().count();
                let target_col = visual_x.min(next_line_chars); // Don't overshoot the next line
                self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                    next_row as u16,
                    target_col as u16,
                ));
            }
        }
    }

    fn visual_move_up(&mut self) {
        let (row, col) = self.textarea.cursor();
        let width = self.editor_width.max(1) as usize;

        // If moving up keeps us on the same logical line
        if col >= width {
            self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                row as u16,
                (col - width) as u16,
            ));
        } else if row > 0 {
            // We are on the top chunk, jump to the previous logical line
            let prev_row = row - 1;
            let prev_line = self
                .textarea
                .lines()
                .get(prev_row)
                .cloned()
                .unwrap_or_default();
            let prev_char_count = prev_line.chars().count();

            let visual_x = col % width;
            // Calculate the start of the last visual chunk on the previous line
            let last_chunk_start = (prev_char_count / width) * width;
            let target_col = (last_chunk_start + visual_x).min(prev_char_count);

            self.textarea.move_cursor(tui_textarea::CursorMove::Jump(
                prev_row as u16,
                target_col as u16,
            ));
        }
    }

    /// Simulates Ratatui's word wrapping to find exact cursor coordinates
    /// Simulates Ratatui's word wrapping to find exact cursor coordinates
    /// Simulates Ratatui's word wrapping to find exact cursor coordinates
    /// Simulates Ratatui's word wrapping to find exact cursor coordinates
    fn calculate_cursor_position(line: &str, cursor_col: usize, width: usize) -> (usize, usize) {
        if width == 0 {
            return (0, 0);
        }
        let chars: Vec<char> = line.chars().collect();

        let mut visual_y = 0;
        let mut current_x = 0;
        let mut i = 0;
        // Tracks if we have placed actual characters (not just spaces) on the current line
        let mut line_has_text = false;

        while i < chars.len() {
            let is_whitespace = chars[i].is_whitespace();
            let mut chunk_width = 0;
            let mut chunk_char_count = 0;
            let mut j = i;

            // Group by whitespace vs non-whitespace to mirror Ratatui's chunking
            while j < chars.len() && chars[j].is_whitespace() == is_whitespace {
                chunk_width += chars[j].width().unwrap_or(0); // Handles emojis/hidden chars
                chunk_char_count += 1;
                j += 1;
            }

            if !is_whitespace {
                // --- WORD CHUNK ---
                // Only wrap if the word exceeds the remaining width AND there is already
                // text on this line. We DO NOT wrap if the line is just leading spaces!
                if current_x + chunk_width > width && line_has_text {
                    visual_y += 1;
                    current_x = 0;
                    let _ = line_has_text;
                }

                line_has_text = true;

                for _ in 0..chunk_char_count {
                    if i == cursor_col {
                        return (visual_y, current_x);
                    }
                    let char_w = chars[i].width().unwrap_or(0);

                    // Hard-break mid-word if it exceeds the boundary
                    if current_x + char_w > width && current_x > 0 {
                        visual_y += 1;
                        current_x = 0;
                        line_has_text = true; // We are still placing the word
                    }
                    current_x += char_w;
                    i += 1;
                }
            } else {
                // --- WHITESPACE CHUNK ---
                for _ in 0..chunk_char_count {
                    if i == cursor_col {
                        return (visual_y, current_x);
                    }
                    let char_w = chars[i].width().unwrap_or(0);

                    if current_x + char_w > width {
                        // Space hits the boundary and is "eaten".
                        if current_x > 0 {
                            visual_y += 1;
                            current_x = 0;
                            line_has_text = false; // New line resets the text flag
                        }
                    } else {
                        current_x += char_w;
                    }
                    i += 1;
                }
            }
        }

        // Return the exact position. (We intentionally do NOT auto-wrap here at the
        // end of the string to ensure height calculations stay perfectly accurate).
        (visual_y, current_x)
    }

    /// Calculates total visual lines a logical line will take up
    fn calculate_visual_lines(line: &str, width: usize) -> usize {
        let (y, _) = Self::calculate_cursor_position(line, usize::MAX, width);
        y + 1
    }

    fn execute_lua_commands(&mut self) -> Result<(), EditorError> {
        // Extract commands so we don't hold the borrow
        let commands: Vec<EditorCommand> = self.command_queue.borrow_mut().drain(..).collect();

        for cmd in commands {
            match cmd {
                EditorCommand::MoveToTop => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Top);
                    self.status = "Moved to top (Lua)".to_string();
                }
                EditorCommand::MoveToBottom => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Bottom);
                    self.status = "Moved to bottom (Lua)".to_string();
                }
                EditorCommand::Echo(msg) => {
                    self.echo(&msg)?;
                }
                EditorCommand::Quit => {
                    self.should_quit = true;
                }
                EditorCommand::StartSearch(search_type) => {
                    // Map the string from Lua to your Rust SearchType enum
                    match search_type.as_str() {
                        "files" => self.start_search(SearchType::Files)?,
                        "tags" => self.start_search(SearchType::Tags)?,
                        "backlinks" => self.start_search(SearchType::Backlinks)?,
                        "sql" => self.start_search(SearchType::CustomSql)?,
                        _ => self.status = format!("Unknown search type: {}", search_type),
                    }
                }
                EditorCommand::MoveToTop => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Top);
                    self.status = "Moved to top".to_string();
                }
                // --- NEW EXECUTIONS ---
                EditorCommand::YankLine => {
                    let row = self.textarea.cursor().0;
                    self.yanked = vec![self.textarea.lines()[row].clone()];
                    self.status = "Yanked line (not undoable)".to_string();
                }
                EditorCommand::DeleteLine => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Head);
                    self.textarea.delete_line_by_end();
                    self.status = "Deleted line".to_string();
                }
                EditorCommand::EnterFileTree => {
                    if self.file_tree.is_empty() {
                        self.file_tree = self.build_root();
                    }
                    self.update_visible();
                    if !self.visible_items.is_empty() {
                        self.tree_state.select(Some(0));
                    }
                    self.mode = Mode::FileTree;
                    self.status = "Entered File Tree mode".to_string();
                }
                EditorCommand::ToggleImageFullScreen => {
                    if self.current_image.is_some() {
                        self.image_full_screen = !self.image_full_screen;
                        self.status = if self.image_full_screen {
                            "Image full screen".to_string()
                        } else {
                            "Image popup".to_string()
                        };
                        self.last_image_area = None;
                    } else {
                        self.status = "No image to toggle full screen".to_string();
                    }
                }
                EditorCommand::StopAudio => {
                    self.stop_audio();
                }
                EditorCommand::ToggleReadMode => {
                    self.read_mode = !self.read_mode;
                    self.status = if self.read_mode {
                        "Read Mode (Wrapping ON)".to_string()
                    } else {
                        "Edit Mode (Wrapping OFF)".to_string()
                    };
                }
                EditorCommand::OpenWikilink(path) => {
                    self.open_wikilink_file(path)?;
                }
                EditorCommand::ProcessTemplate(template) => {
                    self.process_template_command(&template)?;
                }
                EditorCommand::StartSearch(search_type) => match search_type.as_str() {
                    "files" => self.start_search(SearchType::Files)?,
                    "tags" => self.start_search(SearchType::Tags)?,
                    "backlinks" => self.start_search(SearchType::Backlinks)?,
                    "sql" => self.start_search(SearchType::CustomSql)?,
                    _ => self.status = format!("Unknown search type: {}", search_type),
                },
                EditorCommand::MoveUp => self.textarea.move_cursor(tui_textarea::CursorMove::Up),
                EditorCommand::MoveDown => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Down)
                }
                EditorCommand::MoveLeft => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Back)
                }
                EditorCommand::MoveRight => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Forward)
                }
                EditorCommand::MoveWordForward => self
                    .textarea
                    .move_cursor(tui_textarea::CursorMove::WordForward),
                EditorCommand::MoveWordBack => self
                    .textarea
                    .move_cursor(tui_textarea::CursorMove::WordBack),
                EditorCommand::MoveHead => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Head)
                }
                EditorCommand::MoveEnd => self.textarea.move_cursor(tui_textarea::CursorMove::End),
                EditorCommand::MoveToTop => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Top)
                }
                EditorCommand::MoveToBottom => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::Bottom)
                }

                EditorCommand::Undo => {
                    if self.textarea.undo() {
                        self.status = "Undone".to_string();
                    }
                }
                EditorCommand::Redo => {
                    if self.textarea.redo() {
                        self.status = "Redone".to_string();
                    }
                }
                EditorCommand::Save => {
                    self.save_file()?;
                }
                EditorCommand::Paste => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::End);
                    self.textarea.insert_char('\n');
                    self.textarea.insert_str(&self.yanked.join("\n"));
                }
                EditorCommand::InsertLineBelow => {
                    self.textarea.move_cursor(tui_textarea::CursorMove::End);
                    self.textarea.insert_newline();
                    self.mode = Mode::Insert;
                    self.status = "Insert".to_string();
                }
                EditorCommand::EnterMode(mode_str) => match mode_str.as_str() {
                    "insert" => {
                        self.mode = Mode::Insert;
                        self.status = "Insert".to_string();
                    }
                    "visual" => {
                        self.visual_anchor = Some(self.textarea.cursor());
                        self.mode = Mode::Visual;
                        self.status = "Visual".to_string();
                    }
                    "visual_block" => {
                        self.visual_anchor = Some(self.textarea.cursor());
                        self.mode = Mode::VisualBlock;
                        self.status = "Visual Block".to_string();
                    }
                    "command" => {
                        self.prev_mode = Some(Mode::Normal);
                        self.mode = Mode::Command;
                        self.command.clear();
                        self.status = "Command".to_string();
                    }
                    _ => self.status = format!("Unknown mode: {}", mode_str),
                },
                // ... in execute_lua_commands match block ...
                EditorCommand::NavigateBack => {
                    self.navigate_back()?;
                }
                EditorCommand::NavigateForward => {
                    self.navigate_forward()?;
                }
                EditorCommand::ChangeStatus(msg) => {
                    if msg == "clearing_image_flag" {
                        self.clear_image_state();
                    } else {
                        self.status = msg;
                    }
                }
                EditorCommand::Cancel => {
                    // Ensure we get out of the info view
                    self.view = View::Editor;

                    if self.image_full_screen {
                        self.image_full_screen = false;
                        self.status = "Image popup".to_string();
                        self.last_image_area = None; // Force protocol regen
                    } else if !self.key_sequence.is_empty() {
                        self.key_sequence.clear();
                        self.status = "Sequence cancelled".to_string();
                    }
                }
                EditorCommand::FollowLink => {
                    if self.view == View::Editor {
                        let (current_row, current_col) = self.textarea.cursor();
                        let line = self.textarea.lines()[current_row].clone();

                        if let Some(wikilinkline) = self.extract_wikilink(&line, current_col) {
                            if wikilinkline.ends_with(".mp3") {
                                self.playaudio(usize::MAX, &self.music_path.clone())?;
                            } else {
                                self.follow_backlink(usize::MAX)?;
                            }
                        } else if let Some(tag) = self.extract_tag(&line, current_col) {
                            self.load_tag_files(&tag)?;
                            self.mode = Mode::TagFiles;
                        }
                    } else {
                        self.view = View::Editor;
                        self.status = "Normal".to_string();
                    }
                }
                // ... inside execute_lua_commands ...
                EditorCommand::InsertText(text) => {
                    self.textarea.insert_str(&text);
                }
                EditorCommand::SetCursor(row, col) => {
                    self.textarea
                        .move_cursor(tui_textarea::CursorMove::Jump(row as u16, col as u16));
                }
                EditorCommand::SetLines(new_lines) => {
                    // Replace the entire buffer
                    let mut new_textarea = tui_textarea::TextArea::new(new_lines);

                    // Re-apply your default styling macro so it doesn't break visuals
                    set_textarea_delafult_style!(new_textarea);

                    // Keep the cursor roughly where it was, or reset to 0,0
                    let (r, c) = self.textarea.cursor();
                    new_textarea.move_cursor(tui_textarea::CursorMove::Jump(r as u16, c as u16));

                    self.textarea = new_textarea;
                }

                EditorCommand::SetSelection(anchor) => {
                    self.visual_anchor = anchor;
                }
                EditorCommand::SetVirtualText(row, col, text, color) => {
                    self.virtual_texts
                        .entry(row)
                        .or_default()
                        .push((col, text, color));
                }
                EditorCommand::StartCustomSearch(provider, on_select) => {
                    self.search_state.active = true;
                    self.search_state.search_type = SearchType::CustomLua {
                        provider,
                        on_select,
                    };
                    self.search_state.query = String::new();
                    self.search_state.results = Vec::new();
                    self.search_state.list_state = ListState::default();
                    self.mode = Mode::Search;
                    self.view = View::Editor;
                    self.status = "Custom Lua Search".to_string();
                    self.key_sequence.clear();
                    self.update_search_results()?;
                }
                EditorCommand::ShowImage(path, row) => {
                    match self.resolve_image_path(&path) {
                        Ok(full_path) => match image::ImageReader::open(&full_path) {
                            Ok(reader) => match reader.decode() {
                                Ok(dyn_img) => {
                                    self.current_image = Some(dyn_img);
                                    self.current_image_line = Some(row);
                                    self.image_protocol = None; // Force protocol to regenerate
                                    self.last_image_area = None;
                                    self.status = format!("Lua rendered image: {}", path);
                                }
                                Err(e) => self.status = format!("Lua img decode err: {}", e),
                            },
                            Err(e) => self.status = format!("Lua img open err: {}", e),
                        },
                        Err(e) => self.status = format!("Lua img resolve err: {}", e),
                    }
                }
                _ => {} // Optional: A catch-all for any future commands you add to the enum
                        // but haven't implemented here yet.
                        // _ => {}
            }
        }
        Ok(())
    }
    fn event_to_string(event: ratatui::crossterm::event::KeyEvent) -> String {
        use ratatui::crossterm::event::{KeyCode, KeyModifiers};
        let mut s = String::new();

        let has_ctrl = event.modifiers.contains(KeyModifiers::CONTROL);
        let has_alt = event.modifiers.contains(KeyModifiers::ALT);

        if has_ctrl {
            s.push_str("<C-");
        } else if has_alt {
            s.push_str("<A-");
        }

        match event.code {
            KeyCode::Char(c) => {
                if !has_ctrl && !has_alt {
                    return c.to_string();
                }
                s.push(c);
            }
            // Fixed: No brackets here, they are added at the bottom!
            KeyCode::Enter => s.push_str("Enter"),
            KeyCode::Esc => s.push_str("Esc"),
            KeyCode::Up => s.push_str("Up"),
            KeyCode::Down => s.push_str("Down"),
            KeyCode::Left => s.push_str("Left"),
            KeyCode::Right => s.push_str("Right"),
            KeyCode::Home => s.push_str("Home"),
            KeyCode::End => s.push_str("End"),
            _ => s.push_str("Unknown"),
        }

        if has_ctrl || has_alt {
            s.push('>');
            return s;
        }

        // Wrap standalone special keys in <>
        if !matches!(event.code, KeyCode::Char(_)) {
            format!("<{}>", s)
        } else {
            s
        }
    }

    fn get_expanded_line_info(&self, row: usize, original_col: usize) -> (String, usize) {
        let line = self.textarea.lines().get(row).cloned().unwrap_or_default();
        let trimmed_line = line.trim();

        // 1. Check for vid blocks (completely replaces the text)
        if trimmed_line.starts_with("```vid") {
            let spans = Self::build_vid_virtual_text(
                self.textarea.lines(),
                row,
                &self.yt_videos,
                0,
                true, // Always true for measurement to avoid offset stripping
            );
            let expanded_text = spans
                .into_iter()
                .map(|s| s.content.to_string())
                .collect::<String>();
            // Cap cursor to the end of the expanded text
            let mapped_col = original_col.min(expanded_text.chars().count());
            return (expanded_text, mapped_col);
        }

        // 2. Check for standard inline virtual text
        let mut line_v_texts = self.virtual_texts.get(&row).cloned().unwrap_or_default();
        if !line_v_texts.is_empty() {
            line_v_texts.sort_by_key(|v| v.0);
            let mut expanded = String::new();
            let mut mapped_col = original_col;
            let mut v_idx = 0;
            let chars: Vec<char> = line.chars().collect();
            let mut col_tracker = 0;

            while col_tracker < chars.len() {
                while v_idx < line_v_texts.len() && line_v_texts[v_idx].0 <= col_tracker {
                    let v_text = &line_v_texts[v_idx].1;
                    expanded.push_str(v_text);
                    // Crucial fix: Only shift cursor if virtual text strictly BEFORE it (<).
                    // If placed ON the cursor, we want cursor visually sitting BEFORE the overlay text.
                    if line_v_texts[v_idx].0 < original_col {
                        mapped_col += v_text.chars().count();
                    }
                    v_idx += 1;
                }
                expanded.push(chars[col_tracker]);
                col_tracker += 1;
            }
            while v_idx < line_v_texts.len() {
                let v_text = format!(" {}", line_v_texts[v_idx].1);
                expanded.push_str(&v_text);
                if line_v_texts[v_idx].0 < original_col {
                    mapped_col += v_text.chars().count();
                }
                v_idx += 1;
            }
            return (expanded, mapped_col);
        }

        // 3. Normal text line
        (line, original_col)
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.stop_audio();
    }
}
