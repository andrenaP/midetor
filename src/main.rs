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
pub mod lua_api;

use app::App;
use error::EditorError;

fn main() -> Result<(), EditorError> {
    // Define CLI using clap
    let matches = Command::new("midetor")
        .version("1.1.1")
        .about("A terminal-based vim like Markdown editor with Obsidian-like features")
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
            let _ = ProcessCommand::new("stty").arg("echo").status();
        }
    }
    let _guard = TerminalGuard;

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Show)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(file_path, &base_dir, &music_path)?;

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
    drop(app);
    Ok(())
}
