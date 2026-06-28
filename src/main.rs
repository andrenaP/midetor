use clap::{Arg, command};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    crossterm::{
        cursor::Show,
        event::{self, Event, KeyEventKind},
        execute,
        terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
    },
};
use std::env;
use std::io::stdout;
use std::path::Path;

mod app;
mod error;
pub mod lua_api;

use app::App;
use error::EditorError;
use markdown_scanner::scan_markdown_file;

fn main() -> Result<(), EditorError> {
    // Define CLI using clap
    let matches = command!()
        .arg(
            Arg::new("file_path")
                .help("Path to the Markdown file to edit")
                .required(true)
                .index(1),
        )
        .arg(
            Arg::new("base_dir")
                .help("Base directory of the Obsidian vault (defaults to OBSIDIAN_VAULT_PATH or current directory)")
                .index(2)
                .required(false),
        )
        .arg(
            Arg::new("music_folder")
                .help("Music folder")
                .index(3)
                .required(false),
        )
        .get_matches();

    // Extract raw arguments
    let raw_file_path = matches.get_one::<String>("file_path").unwrap();
    let music_path = matches
        .get_one::<String>("music_folder")
        .map(|s| s.to_string())
        .or_else(|| env::var("MUSIC_FOLDER").ok())
        .or_else(|| env::var("musik_folder").ok())
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());

    let raw_base_dir = matches
        .get_one::<String>("base_dir")
        .map(|s| s.to_string())
        .or_else(|| env::var("OBSIDIAN_VAULT_PATH").ok())
        .or_else(|| env::var("Obsidian_valt_main_path").ok())
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());

    // 1. Resolve and canonicalize the base directory
    let base_dir_path = Path::new(&raw_base_dir);
    if !base_dir_path.exists() {
        return Err(EditorError::InvalidPath(format!(
            "Base directory '{}' does not exist",
            raw_base_dir
        )));
    }
    let base_dir_canonical = base_dir_path
        .canonicalize()
        .map_err(|e| EditorError::InvalidPath(format!("Failed to resolve base dir: {}", e)))?;
    let base_dir_str = base_dir_canonical.to_string_lossy().to_string();

    // 2. Resolve the full file path safely
    // If raw_file_path is absolute, join() uses it directly.
    // If it's relative, it smartly appends to the canonical base_dir.
    let full_file_path = base_dir_canonical.join(raw_file_path);

    // Ensure the target file (and any nested parent directories) exists
    if !full_file_path.exists() {
        if let Some(parent) = full_file_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&full_file_path, "")?;
    }

    // Canonicalize the file path to strip any `./` or `../`
    let full_file_path_canonical = full_file_path
        .canonicalize()
        .map_err(|e| EditorError::InvalidPath(format!("Failed to resolve file path: {}", e)))?;
    let full_file_path_str = full_file_path_canonical.to_string_lossy().to_string();

    // 3. Database check and Scanner initialization
    let db_path = base_dir_canonical.join("markdown_data.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    if !db_path.exists() {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| EditorError::Scanner(format!("Failed to start runtime: {}", e)))?;

        // Pass the absolute db_path_str here!
        let scan_result = rt.block_on(scan_markdown_file(
            &full_file_path_str,
            &base_dir_str,
            &db_path_str,
            false,
        ));
        if let Err(e) = scan_result {
            return Err(EditorError::Scanner(e.to_string()));
        }
    }

    // Ensure terminal cleanup on exit
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(stdout(), LeaveAlternateScreen, Show);
        }
    }
    let _guard = TerminalGuard;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Show)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Pass the perfectly sanitized, absolute paths into the App
    let mut app = App::new(&full_file_path_str, &base_dir_str, &music_path)?;

    while !app.should_quit {
        app.render(&mut terminal)?;

        let evt = event::read()?;

        match evt {
            Event::Paste(s) => {
                app.handle_paste(s)?;
            }
            Event::Key(key_event) => {
                if key_event.kind == KeyEventKind::Press || key_event.kind == KeyEventKind::Repeat {
                    app.handle_input(key_event)?;
                }
            }
            Event::Resize(_, _) => {}
            _ => {}
        }
    }
    drop(app);
    Ok(())
}
