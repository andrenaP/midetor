use crossterm::{
    cursor::Show,
    event::{self, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::stdout;
use std::process::Command;

mod app;
mod error;

use app::App;
use error::EditorError;

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
