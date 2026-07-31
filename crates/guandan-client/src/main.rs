//! Guandan terminal client (ratatui).

mod app;
mod net;
mod settings;
mod ui;

use anyhow::Result;
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::time::Duration;

use app::{App, Screen};
use net::NetHandle;
use settings::Settings;

#[derive(Parser, Debug)]
#[command(name = "guandan", about = "掼蛋终端客户端 Guandan TUI")]
struct Args {
    /// WebSocket server URL
    #[arg(long, default_value = "ws://127.0.0.1:9100")]
    server: String,
    /// Seconds to display another player's play (default: 3, or settings file).
    #[arg(long, env = "GUANDAN_REVEAL_SECS")]
    play_reveal_secs: Option<u64>,
    /// Fallback turn timer display seconds (server sends the real limit).
    #[arg(long, env = "GUANDAN_TURN_SECS")]
    turn_timeout_secs: Option<u64>,
    /// Write current settings to ~/.config/guandan/settings.toml and exit.
    #[arg(long)]
    save_settings: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut settings = Settings::load();
    if let Some(v) = args.play_reveal_secs {
        settings.play_reveal_secs = v;
    }
    if let Some(v) = args.turn_timeout_secs {
        settings.turn_timeout_secs = v;
    }
    if args.save_settings {
        settings.save()?;
        println!("Saved settings to {}", Settings::config_path().display());
        println!(
            "  play_reveal_secs = {}\n  turn_timeout_secs = {}",
            settings.play_reveal_secs, settings.turn_timeout_secs
        );
        return Ok(());
    }

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &args.server, settings).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    server: &str,
    settings: Settings,
) -> Result<()> {
    let (net, mut incoming) = NetHandle::connect(server).await?;
    let mut app = App::new(net, settings);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        while let Ok(msg) = incoming.try_recv() {
            app.on_server(msg);
        }

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
                    break;
                }
                if app.on_key(key.code) {
                    break;
                }
            }
        }

        if app.should_quit {
            break;
        }

        app.tick();
    }

    let _ = Screen::Lobby;
    Ok(())
}
