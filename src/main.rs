use crossterm::{
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::{
    prelude::{CrosstermBackend, Frame, Terminal},
    text::Line,
    widgets::{Block, Borders, Paragraph, Scrollbar, ScrollbarState},
};
use std::io::stdout;
use tokio::sync::{broadcast, mpsc};
use tokio_stream::StreamExt;

#[derive(Debug, Clone, Copy)]
struct Shutdown;

enum Event {
    Key(char),
    ScrollDown,
    ScrollUp,
    LineBreak,
    Exit,
}

#[derive(Debug, Default)]
struct AppState {
    scroll_state: ScrollbarState,
    scroll_position: usize,
    line_count: usize,
    text: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    enable_raw_mode()?;
    stdout().execute(EnterAlternateScreen)?;
    let (event_sender, event_receiver) = mpsc::channel(16);
    let (shutdown_sender, shutdown_receiver) = broadcast::channel(1);
    let poll_task = tokio::spawn(poll_keys(event_sender, shutdown_receiver));
    let draw_task = tokio::spawn(draw_loop(event_receiver, shutdown_sender));

    let polling_result = poll_task.await?;
    let drawing_result = draw_task.await?;

    disable_raw_mode()?;
    stdout().execute(LeaveAlternateScreen)?;

    if let Err(e) = polling_result {
        println!("Polling error: {e:?}");
    }
    if let Err(e) = drawing_result {
        println!("Drawing error: {e:?}");
    }

    Ok(())
}

async fn draw_loop(
    mut stream: mpsc::Receiver<Event>,
    shutdown: broadcast::Sender<Shutdown>,
) -> anyhow::Result<()> {
    let mut terminal = Terminal::new(CrosstermBackend::new(stdout()))?;
    let mut shutdown_receiver = shutdown.subscribe();
    let refresh_time = std::time::Duration::from_millis(100);
    let mut state = AppState::default();
    state.text.push_str("Hello, World!\n");
    state.line_count = 1;

    loop {
        let maybe_event = tokio::select! {
            x = stream.recv() => x,
            _ = shutdown_receiver.recv() => break,
            _ = tokio::time::sleep(refresh_time) => None,
        };
        match maybe_event {
            Some(Event::Exit) => {
                shutdown.send(Shutdown).ok();
                break;
            }
            Some(Event::Key(c)) => {
                state.text.push(c);
            }
            Some(Event::LineBreak) => {
                state.text.push('\n');
                state.line_count += 1;
            }
            Some(Event::ScrollDown) => {
                state.scroll_state.next();
                state.scroll_position = state
                    .scroll_position
                    .saturating_add(1)
                    .clamp(0, state.line_count);
            }
            Some(Event::ScrollUp) => {
                state.scroll_state.prev();
                state.scroll_position = state
                    .scroll_position
                    .saturating_sub(1)
                    .clamp(0, state.line_count);
            }
            None => (),
        }
        terminal.draw(|frame| ui(frame, &mut state))?;
    }

    Ok(())
}

async fn poll_keys(
    sender: mpsc::Sender<Event>,
    mut shutdown: broadcast::Receiver<Shutdown>,
) -> anyhow::Result<()> {
    let mut stream = crossterm::event::EventStream::new();
    loop {
        let maybe_event = tokio::select! {
            x = stream.next() => x,
            _ = shutdown.recv() => break,
        };
        if let Some(x) = maybe_event {
            if let crossterm::event::Event::Key(key) = x? {
                let event = if key.kind == crossterm::event::KeyEventKind::Press {
                    match key.code {
                        crossterm::event::KeyCode::Char('q') => Some(Event::Exit),
                        crossterm::event::KeyCode::Char(c) => Some(Event::Key(c)),
                        crossterm::event::KeyCode::Up => Some(Event::ScrollUp),
                        crossterm::event::KeyCode::Down => Some(Event::ScrollDown),
                        crossterm::event::KeyCode::Enter => Some(Event::LineBreak),
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(e) = event {
                    sender.send(e).await?;
                }
            }
        }
    }
    Ok(())
}

fn ui(frame: &mut Frame, state: &mut AppState) {
    state.scroll_state = state.scroll_state.content_length(state.line_count);

    let render_lines: Vec<Line> = state
        .text
        .lines()
        .skip(state.scroll_position)
        .map(Into::into)
        .collect();

    frame.render_widget(
        Paragraph::new(render_lines)
            .block(Block::default().title("Greeting").borders(Borders::ALL)),
        frame.area(),
    );
    frame.render_stateful_widget(Scrollbar::default(), frame.area(), &mut state.scroll_state);
}
