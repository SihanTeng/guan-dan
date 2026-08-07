//! Guandan terminal client (ratatui).

mod app;
mod net;
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
use tokio::sync::mpsc::error::TryRecvError;

use app::App;
use net::NetHandle;

#[derive(Parser, Debug)]
#[command(name = "guandan", about = "掼蛋终端客户端 Guandan TUI")]
struct Args {
    /// WebSocket server URL
    #[arg(long, default_value = "ws://127.0.0.1:9100")]
    server: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Restore the terminal on panic — otherwise the shell is left stuck in
    // raw mode / alternate screen.
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        default_hook(info);
    }));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let result = run(&mut terminal, &args.server).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, server: &str) -> Result<()> {
    let (net, mut incoming) = NetHandle::connect(server).await?;
    let mut app = App::new(net);

    loop {
        terminal.draw(|f| ui::draw(f, &app))?;

        loop {
            match incoming.try_recv() {
                Ok(msg) => app.on_server(msg),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    app.on_disconnect();
                    break;
                }
            }
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

    Ok(())
}
