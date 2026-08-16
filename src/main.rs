use clap::{Arg, ArgAction, Command};
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
use markdown_scanner::{
    clean_orphaned_files, delete_asset_file, delete_markdown_file, register_asset_file,
    scan_markdown_file,
};

fn main() -> Result<(), EditorError> {
    // Define unified CLI using clap with subcommands
    let matches = Command::new("midetor")
        .version("1.2.0")
        .about("Markdown editor and scanner combined")
        // --- EDITOR ARGUMENTS (Default behavior) ---
        .arg(
            Arg::new("file_path")
                .help("Path to the Markdown file to edit")
                .index(1),
        )
        .arg(
            Arg::new("base_dir")
                .help("Base directory of the Obsidian vault (defaults to OBSIDIAN_VAULT_PATH or current directory)")
                .index(2)
        )
        .arg(
            Arg::new("music_folder")
                .help("Music folder")
                .index(3)
        )
        // --- SCANNER SUBCOMMAND ---
        .subcommand(
            Command::new("scan")
                .about("Run the standalone markdown scanner")
                .arg(Arg::new("file").required(true))
                .arg(Arg::new("base_dir").required(true))
                .arg(
                    Arg::new("database")
                        .long("database")
                        .short('d')
                        .default_value("markdown_data.db"),
                )
                .arg(
                    Arg::new("json-only")
                        .long("json-only")
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("delete")
                        .long("delete")
                        .short('x')
                        .action(ArgAction::SetTrue),
                )
                .arg(
                    Arg::new("clean")
                        .long("clean")
                        .short('c')
                        .action(clap::ArgAction::SetTrue),
                ),
        )
        .get_matches();

    // 1. Check if we are running the SCANNER

    if let Some(scan_matches) = matches.subcommand_matches("scan") {
        env_logger::init(); // Initialize logger only for the scanner if needed

        let file_path = scan_matches.get_one::<String>("file").unwrap();
        let base_dir = scan_matches.get_one::<String>("base_dir").unwrap();
        let raw_db_path = scan_matches.get_one::<String>("database").unwrap();
        let json_only = scan_matches.get_flag("json-only");
        let delete_flag = scan_matches.get_flag("delete");
        let clean_flag = scan_matches.get_flag("clean");

        // If json-only is true, use an in-memory SQLite database to prevent creating a file on disk.
        let db_path = if json_only { ":memory:" } else { raw_db_path };
        if clean_flag {
            clean_orphaned_files(base_dir, db_path)
                .map_err(|e| EditorError::Scanner(e.to_string()))?;
        }
        let is_markdown = Path::new(file_path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.eq_ignore_ascii_case("md"))
            .unwrap_or(false);

        // Spin up the tokio runtime for the async scanner code
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| EditorError::Scanner(format!("Failed to start runtime: {}", e)))?;

        rt.block_on(async {
            if delete_flag {
                if is_markdown {
                    delete_markdown_file(file_path, base_dir, db_path)
                        .map_err(|e| EditorError::Scanner(e.to_string()))?;
                } else {
                    delete_asset_file(file_path, base_dir, db_path)
                        .map_err(|e| EditorError::Scanner(e.to_string()))?;
                }
            } else {
                if is_markdown {
                    if let Some(json) = scan_markdown_file(file_path, base_dir, db_path, json_only)
                        .await
                        .map_err(|e| EditorError::Scanner(e.to_string()))?
                    {
                        println!("{}", json);
                    }
                } else {
                    // It's an image or another asset, register it in the folders/files tables
                    register_asset_file(file_path, base_dir, db_path)
                        .map_err(|e| EditorError::Scanner(e.to_string()))?;
                }
            }
            Ok::<(), EditorError>(())
        })?;

        return Ok(()); // Exit early; we don't want to open the TUI editor
    }

    // 2. Otherwise, run the TUI EDITOR

    // Ensure the user provided a file path (since it's only optional to allow subcommands)
    let raw_file_path = match matches.get_one::<String>("file_path") {
        Some(path) => path,
        None => {
            eprintln!("Error: 'file_path' is required unless using the 'scan' subcommand.");
            eprintln!("Usage: midetor <file_path> [base_dir] [music_folder]");
            eprintln!("       midetor scan <file> <base_dir> [OPTIONS]");
            std::process::exit(1);
        }
    };

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

    // Resolve and canonicalize the base directory
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

    // Resolve the full file path safely
    let full_file_path = base_dir_canonical.join(raw_file_path);

    // Ensure the target file (and any nested parent directories) exists
    if !full_file_path.exists() {
        if let Some(parent) = full_file_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| EditorError::InvalidPath(e.to_string()))?;
        }
        std::fs::write(&full_file_path, "").map_err(|e| EditorError::InvalidPath(e.to_string()))?;
    }

    // Canonicalize the file path to strip any `./` or `../`
    let full_file_path_canonical = full_file_path
        .canonicalize()
        .map_err(|e| EditorError::InvalidPath(format!("Failed to resolve file path: {}", e)))?;
    let full_file_path_str = full_file_path_canonical.to_string_lossy().to_string();

    // Database check and Scanner initialization
    let db_path = base_dir_canonical.join("markdown_data.db");
    let db_path_str = db_path.to_string_lossy().to_string();
    if !db_path.exists() {
        let is_markdown = Path::new(&full_file_path_str)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.eq_ignore_ascii_case("md"))
            .unwrap_or(false);

        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| EditorError::Scanner(format!("Failed to start runtime: {}", e)))?;

        // If the initialized editor file is markdown, scan it to initialize DB.
        // Otherwise, register it as an asset.
        let init_result = rt.block_on(async {
            if is_markdown {
                scan_markdown_file(&full_file_path_str, &base_dir_str, &db_path_str, false)
                    .await
                    .map(|_| ())
            } else {
                register_asset_file(&full_file_path_str, &base_dir_str, &db_path_str)
            }
        });

        if let Err(e) = init_result {
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

    enable_raw_mode().map_err(|e| EditorError::InvalidPath(e.to_string()))?; // map std::io::Error if needed
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, Show).unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    // Pass the perfectly sanitized, absolute paths into the App
    let mut app = App::new(&full_file_path_str, &base_dir_str, &music_path)?;

    while !app.should_quit {
        app.render(&mut terminal)?;

        let evt = event::read().unwrap();

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
