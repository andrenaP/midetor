use clap::{Arg, Command};
use crossterm::{
    cursor::Show,
    event::{self, Event},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::env;
use std::io::stdout;
use std::path::Path;
use std::process::Command as ProcessCommand;

mod app;
mod error;

use app::App;
use error::EditorError;

fn main() -> Result<(), EditorError> {
    // Define CLI using clap
    let matches = Command::new("midetor")
        .version("1.0.18")
        .about("A terminal-based vim like Markdown editor with Obsidian-like features")
        .arg(
            Arg::new("file_path")
                .help("Path to the Markdown file to edit")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("base_dir")
                .help("Base directory of the Obsidian vault (defaults to Obsidian_valt_main_path or current directory)")
                .index(2)
                .required(false),
        )
        .get_matches();

    // Extract file_path
    let file_path = matches.get_one::<String>("file_path").unwrap();

    // Determine base_dir: use provided, then OBSIDIAN_VAULT_MAIN_PATH, then current directory
    let base_dir = matches
        .get_one::<String>("base_dir")
        .map(|s| s.to_string())
        .or_else(|| env::var("Obsidian_valt_main_path").ok())
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());

    // Ensure base_dir exists
    if !Path::new(&base_dir).exists() {
        return Err(EditorError::InvalidPath(format!(
            "Base directory '{}' does not exist",
            base_dir
        )));
    }

    // Check for and initialize markdown_data.db if it doesn't exist
    let db_path = Path::new(&base_dir).join("markdown_data.db");
    if !db_path.exists() {
        // Create the database and initialize schema
        let db = rusqlite::Connection::open(&db_path)?;
        db.execute(
            "CREATE TABLE IF NOT EXISTS folders (
                    id INTEGER PRIMARY KEY,
                    path TEXT UNIQUE
                )",
            [],
        )?;

        // Files table
        db.execute(
                "CREATE TABLE IF NOT EXISTS files (
                    id INTEGER PRIMARY KEY,
                    path TEXT UNIQUE,
                    file_name TEXT,
                    folder_id INTEGER,
                    metadata TEXT DEFAULT '{}',
                    FOREIGN KEY(folder_id) REFERENCES folders(id) ON DELETE CASCADE  -- Optional: cascades if deleting folders
                )",
                [],
            )?;

        // Tags table
        db.execute(
            "CREATE TABLE IF NOT EXISTS tags (
                    id INTEGER PRIMARY KEY,
                    tag TEXT UNIQUE
                )",
            [],
        )?;

        // File_tags table (cascade on file_id, but not on tag_id)
        db.execute(
                "CREATE TABLE IF NOT EXISTS file_tags (
                    file_id INTEGER,
                    tag_id INTEGER,
                    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,  -- Auto-delete tags for this file
                    FOREIGN KEY(tag_id) REFERENCES tags(id),                      -- No cascade: keep tags
                    UNIQUE(file_id, tag_id)
                )",
                [],
            )?;

        // Backlinks table (cascade on both file_id and backlink_id for bidirectional cleanup)
        db.execute(
                "CREATE TABLE IF NOT EXISTS backlinks (
                    id INTEGER PRIMARY KEY,
                    backlink TEXT,
                    backlink_id INTEGER,
                    file_id INTEGER,
                    FOREIGN KEY(file_id) REFERENCES files(id) ON DELETE CASCADE,             -- Auto-delete if target file deleted
                    FOREIGN KEY(backlink_id) REFERENCES files(id) ON DELETE CASCADE,         -- Auto-delete if source file deleted
                    UNIQUE(backlink_id, file_id, backlink)
                )",
                [],
            )?;
        // Ensure markdown-scanner is run to populate the database
        let output = ProcessCommand::new("markdown-scanner")
            .arg(file_path)
            .arg(&base_dir)
            .output()?;
        if !output.status.success() {
            let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
            return Err(EditorError::Scanner(error_msg));
        }
    }

    // Ensure terminal cleanup on exit
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(stdout(), LeaveAlternateScreen, Show);
            let _ = ProcessCommand::new("stty").arg("echo").status();
        }
    }
    let _guard = TerminalGuard;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Show)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(file_path, &base_dir)?;

    while !app.should_quit {
        app.render(&mut terminal)?;
        match event::read()? {
            Event::Paste(s) => {
                app.handle_paste(s)?;
            }
            Event::Key(event) => {
                let ratatui_event = ratatui::crossterm::event::KeyEvent {
                    code: match event.code {
                        crossterm::event::KeyCode::Char(c) => {
                            ratatui::crossterm::event::KeyCode::Char(c)
                        }
                        crossterm::event::KeyCode::Enter => {
                            ratatui::crossterm::event::KeyCode::Enter
                        }
                        crossterm::event::KeyCode::Backspace => {
                            ratatui::crossterm::event::KeyCode::Backspace
                        }
                        crossterm::event::KeyCode::Esc => ratatui::crossterm::event::KeyCode::Esc,
                        crossterm::event::KeyCode::Left => ratatui::crossterm::event::KeyCode::Left,
                        crossterm::event::KeyCode::Right => {
                            ratatui::crossterm::event::KeyCode::Right
                        }
                        crossterm::event::KeyCode::Up => ratatui::crossterm::event::KeyCode::Up,
                        crossterm::event::KeyCode::Down => ratatui::crossterm::event::KeyCode::Down,
                        crossterm::event::KeyCode::Home => ratatui::crossterm::event::KeyCode::Home,
                        crossterm::event::KeyCode::End => ratatui::crossterm::event::KeyCode::End,
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

            Event::Resize(_, _) => {}

            _ => {}
        }
    }

    Ok(())
}
