mod data;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use data::ThemeData;
use std::{env, io};
use ratatui::{
    prelude::*,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
};

struct App {
    themes: Vec<String>,
    state: ListState,
    status_message: String,
}

impl App {
    fn new(themes: Vec<String>) -> App {
        let mut state = ListState::default();
        state.select(Some(0));
        App {
            themes,
            state,
            status_message: String::from("Press Enter to apply theme, q to quit"),
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.themes.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.themes.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    // Check for force refresh flag
    let force_refresh = env::args().any(|arg| arg == "--force-refresh");

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Initialize theme data
    let theme_data = match ThemeData::new() {
        Ok(td) => td,
        Err(e) => {
            disable_raw_mode()?;
            println!("Error initializing theme data: {}", e);
            return Ok(());
        }
    };

    // Ensure theme directory exists
    if let Err(e) = theme_data.ensure_theme_dir() {
        disable_raw_mode()?;
        println!("Error checking theme directory: {}", e);
        return Ok(());
    }

    // Fetch available themes
    let themes = match ThemeData::fetch_themes(force_refresh).await {
        Ok(themes) => themes,
        Err(e) => {
            disable_raw_mode()?;
            println!("Error fetching themes: {}", e);
            return Ok(());
        }
    };

    let mut app = App::new(themes);
    let res = run_app(&mut terminal, &mut app, theme_data);

    // Restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("Error: {}", err);
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    theme_data: ThemeData,
) -> io::Result<()> {
    loop {
        terminal.draw(|frame| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),  // Status
                    Constraint::Min(1),     // List
                ])
                .split(frame.size());

            // Status message
            let status = Paragraph::new(app.status_message.clone())
                .block(Block::default().borders(Borders::ALL).title("Status"));
            frame.render_widget(status, chunks[0]);

            // Theme list
            let items: Vec<ListItem> = app
                .themes
                .iter()
                .map(|theme| {
                    ListItem::new(Line::from(vec![Span::styled(
                        theme,
                        Style::default().add_modifier(Modifier::BOLD),
                    )]))
                })
                .collect();

            let themes = List::new(items)
                .block(Block::default().borders(Borders::ALL).title("Themes"))
                .highlight_style(
                    Style::default()
                        .bg(Color::Green)
                        .add_modifier(Modifier::BOLD),
                )
                .highlight_symbol("> ");

            frame.render_stateful_widget(themes, chunks[1], &mut app.state);
        })?;

        if let Event::Key(key) = event::read()? {
            if key.kind == KeyEventKind::Press {
                match key.code {
                    KeyCode::Char('q') => return Ok(()),
                    KeyCode::Down | KeyCode::Char('j') => app.next(),
                    KeyCode::Up | KeyCode::Char('k') => app.previous(),
                    KeyCode::Enter => {
                        if let Some(selected) = app.state.selected() {
                            let theme = &app.themes[selected];
                            match theme_data.update_config(theme) {
                                Ok(_) => {
                                    app.status_message = format!("Successfully applied theme: {}", theme);
                                }
                                Err(e) => {
                                    app.status_message = format!("Error updating config: {}", e);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
} 