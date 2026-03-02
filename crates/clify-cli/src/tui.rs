//! Interactive TUI for Clify using ratatui.

use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    execute,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Terminal,
};
use std::io;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Screen {
    Main,
    Init,
    Scan,
    Validate,
    Generate,
    Build,
}

struct App {
    screen: Screen,
    main_menu: MenuState,
    status_message: String,
    should_quit: bool,
    // Action results
    action_result: Option<ActionResult>,
}

struct MenuState {
    items: Vec<(&'static str, &'static str)>,
    state: ListState,
}

impl MenuState {
    fn new(items: Vec<(&'static str, &'static str)>) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        Self { items, state }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => (i + 1) % self.items.len(),
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 { self.items.len() - 1 } else { i - 1 }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn selected(&self) -> usize {
        self.state.selected().unwrap_or(0)
    }
}

enum ActionResult {
    Success(String),
    Error(String),
}

impl App {
    fn new() -> Self {
        Self {
            screen: Screen::Main,
            main_menu: MenuState::new(vec![
                ("🔍 Scan API Spec", "Generate .clify.yaml from an OpenAPI or Swagger spec"),
                ("✅ Validate Spec", "Check a .clify.yaml file for errors"),
                ("⚙️  Generate CLI", "Create a Rust CLI project from a .clify.yaml"),
                ("🔨 Build CLI", "Compile the generated CLI project"),
                ("📄 Export Schema", "Export JSON Schema for .clify.yaml"),
                ("🚪 Quit", "Exit Clify"),
            ]),
            status_message: String::new(),
            should_quit: false,
            action_result: None,
        }
    }
}

pub fn run_tui() -> io::Result<()> {
    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new();

    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        if let Event::Key(key) = event::read()? {
            if key.kind != KeyEventKind::Press {
                continue;
            }
            match app.screen {
                Screen::Main => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => app.should_quit = true,
                    KeyCode::Up | KeyCode::Char('k') => app.main_menu.previous(),
                    KeyCode::Down | KeyCode::Char('j') => app.main_menu.next(),
                    KeyCode::Enter => {
                        match app.main_menu.selected() {
                            0 => {
                                app.status_message = "Use: clify scan --from openapi <spec-file>".to_string();
                                app.action_result = Some(ActionResult::Success(
                                    "Scan command:\n\n  clify scan --from openapi ./api-spec.yaml\n  clify scan --from swagger ./swagger.json\n  clify scan --from openapi https://api.example.com/openapi.json".to_string()
                                ));
                            }
                            1 => {
                                app.status_message = "Use: clify validate <spec.clify.yaml>".to_string();
                                app.action_result = Some(ActionResult::Success(
                                    "Validate command:\n\n  clify validate my-api.clify.yaml\n\nChecks for:\n  • Valid meta (name, version)\n  • Transport config\n  • Auth configuration\n  • Command/param consistency\n  • Path interpolation\n  • Enum values".to_string()
                                ));
                            }
                            2 => {
                                app.status_message = "Use: clify generate <spec> --output <dir>".to_string();
                                app.action_result = Some(ActionResult::Success(
                                    "Generate command:\n\n  clify generate my-api.clify.yaml --output ./out\n\nProduces a complete Rust CLI project:\n  • Cargo.toml with all dependencies\n  • Clap-based command structure\n  • Auth management (login/status/logout)\n  • Config management (set/get/list/reset)\n  • Output formatting (JSON/table/CSV)\n  • Dry-run mode".to_string()
                                ));
                            }
                            3 => {
                                app.status_message = "Use: clify build [--release]".to_string();
                                app.action_result = Some(ActionResult::Success(
                                    "Build command:\n\n  cd <generated-project>\n  clify build --release\n\nOr directly:\n  cargo build --release".to_string()
                                ));
                            }
                            4 => {
                                // Actually export schema
                                let schema = clify_core::schema::generate_json_schema();
                                let path = "clify-spec.schema.json";
                                match std::fs::write(path, &schema) {
                                    Ok(_) => {
                                        app.status_message = format!("Schema exported to {}", path);
                                        app.action_result = Some(ActionResult::Success(
                                            format!("✅ JSON Schema written to {}\n\nAdd to your .clify.yaml for IDE support:\n\n  # yaml-language-server: $schema=./{}",path, path)
                                        ));
                                    }
                                    Err(e) => {
                                        app.action_result = Some(ActionResult::Error(e.to_string()));
                                    }
                                }
                            }
                            5 => app.should_quit = true,
                            _ => {}
                        }
                    }
                    _ => {}
                },
                _ => match key.code {
                    KeyCode::Esc | KeyCode::Char('q') => {
                        app.screen = Screen::Main;
                        app.action_result = None;
                    }
                    _ => {}
                },
            }
        }

        if app.should_quit {
            break;
        }
    }

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    Ok(())
}

fn ui(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),  // Header
            Constraint::Min(10),   // Content
            Constraint::Length(3), // Footer
        ])
        .split(f.area());

    // Header
    let header = Paragraph::new(Line::from(vec![
        Span::styled(" Clify ", Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD)),
        Span::raw("— "),
        Span::styled("make your software cliable", Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::BOTTOM).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(header, chunks[0]);

    // Content
    match app.screen {
        Screen::Main => render_main_menu(f, app, chunks[1]),
        _ => {}
    }

    // Footer
    let footer_text = if !app.status_message.is_empty() {
        app.status_message.clone()
    } else {
        "↑↓ navigate  ⏎ select  q quit".to_string()
    };
    let footer = Paragraph::new(Line::from(vec![
        Span::styled(footer_text, Style::default().fg(Color::DarkGray)),
    ]))
    .block(Block::default().borders(Borders::TOP).border_style(Style::default().fg(Color::DarkGray)));
    f.render_widget(footer, chunks[2]);
}

fn render_main_menu(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(40),
            Constraint::Percentage(60),
        ])
        .split(area);

    // Menu list
    let items: Vec<ListItem> = app.main_menu.items.iter().map(|(label, _desc)| {
        ListItem::new(Line::from(Span::raw(*label)))
    }).collect();

    let menu = List::new(items)
        .block(Block::default().title(" Menu ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
        .highlight_style(Style::default().fg(Color::Cyan).add_modifier(Modifier::BOLD))
        .highlight_symbol("▶ ");

    f.render_stateful_widget(menu, chunks[0], &mut app.main_menu.state);

    // Right panel — description or result
    let right_content = if let Some(ref result) = app.action_result {
        match result {
            ActionResult::Success(msg) => {
                Paragraph::new(Text::from(msg.as_str()))
                    .style(Style::default().fg(Color::Green))
                    .wrap(Wrap { trim: false })
                    .block(Block::default().title(" Result ").borders(Borders::ALL).border_style(Style::default().fg(Color::Green)))
            }
            ActionResult::Error(msg) => {
                Paragraph::new(Text::from(msg.as_str()))
                    .style(Style::default().fg(Color::Red))
                    .wrap(Wrap { trim: false })
                    .block(Block::default().title(" Error ").borders(Borders::ALL).border_style(Style::default().fg(Color::Red)))
            }
        }
    } else {
        let desc = app.main_menu.items[app.main_menu.selected()].1;
        Paragraph::new(Text::from(desc))
            .style(Style::default().fg(Color::White))
            .wrap(Wrap { trim: false })
            .block(Block::default().title(" Description ").borders(Borders::ALL).border_style(Style::default().fg(Color::DarkGray)))
    };

    f.render_widget(right_content, chunks[1]);
}
