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
use std::process::Command as ProcessCommand;

mod app;
mod error;
pub mod lua_api;

use app::App;
use error::EditorError;

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

    // Extract file_path
    let file_path = matches.get_one::<String>("file_path").unwrap();
    let music_path = matches
        .get_one::<String>("music_folder")
        .map(|s| s.to_string())
        .or_else(|| env::var("MUSIC_FOLDER").ok()) // Standardized
        .or_else(|| env::var("musik_folder").ok()) // Fallback
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());

    let base_dir = matches
        .get_one::<String>("base_dir")
        .map(|s| s.to_string())
        .or_else(|| env::var("OBSIDIAN_VAULT_PATH").ok()) // Standardized
        .or_else(|| env::var("Obsidian_valt_main_path").ok()) // Fallback
        .unwrap_or_else(|| env::current_dir().unwrap().to_string_lossy().to_string());

    // Ensure base_dir exists
    if !Path::new(&base_dir).exists() {
        return Err(EditorError::InvalidPath(format!(
            "Base directory '{}' does not exist",
            base_dir
        )));
    }

    // Ensure the target file actually exists on the disk
    let full_file_path = Path::new(&base_dir).join(file_path);
    if !full_file_path.exists() {
        std::fs::write(&full_file_path, "")?;
    }

    // Check for and initialize markdown_data.db if it doesn't exist
    let db_path = Path::new(&base_dir).join("markdown_data.db");
    if !db_path.exists() {
        ProcessCommand::new("markdown-scanner")
            .arg(file_path)
            .arg(&base_dir)
            .output()?;
    }

    // 2. Force the scanner to run for this specific file on startup.
    // This ensures it is always in the database before App::new runs.
    // let output = ProcessCommand::new("markdown-scanner")
    //     .arg(&full_file_path)
    //     .arg(&base_dir)
    //     .output()?;

    // if !output.status.success() {
    //     let error_msg = String::from_utf8_lossy(&output.stderr).into_owned();
    //     return Err(EditorError::Scanner(error_msg));
    // }

    // Ensure terminal cleanup on exit
    struct TerminalGuard;
    impl Drop for TerminalGuard {
        fn drop(&mut self) {
            let _ = disable_raw_mode();
            let _ = execute!(stdout(), LeaveAlternateScreen, Show);
            // let _ = ProcessCommand::new("stty").arg("echo").status();
        }
    }
    let _guard = TerminalGuard;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Show)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let full_file_path_str = full_file_path.to_string_lossy().to_string();

    let mut app = App::new(&full_file_path_str, &base_dir, &music_path)?;

    while !app.should_quit {
        // 1. Draw the UI
        app.render(&mut terminal)?;

        // 2. Block until the user does *something* (Zero CPU usage while idle)
        let evt = event::read()?;

        // 3. Process the event
        match evt {
            Event::Paste(s) => {
                app.handle_paste(s)?;
            }
            Event::Key(key_event) => {
                if key_event.kind == KeyEventKind::Press || key_event.kind == KeyEventKind::Repeat {
                    app.handle_input(key_event)?;
                }
            }
            Event::Resize(_, _) => {
                // Do nothing here. The loop will restart and automatically
            }
            _ => {}
        }
    }
    drop(app);
    Ok(())
}
