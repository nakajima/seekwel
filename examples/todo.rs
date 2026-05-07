//! A tiny todo-list app built on top of seekwel.
//!
//! Try it with:
//!
//! ```bash
//! cargo run --example todo
//! cargo run --example todo -- tui
//! cargo run --example todo -- seed
//! cargo run --example todo -- list
//! cargo run --example todo -- add "buy oat milk"
//! cargo run --example todo -- done 1
//! cargo run --example todo -- clear-done
//! cargo run --example todo -- stats
//! ```

use std::env;
use std::fs;
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::{Frame, Terminal};
use seekwel::{
    Comparison,
    connection::Connection,
    prelude::*,
    schema::{ApplyMode, Plan, PlanOp, SchemaBuilder},
};

#[seekwel::model]
struct Todo {
    id: u64,
    title: String,
    done: bool,
    notes: Option<String>,
}

type TodoTerminal = Terminal<CrosstermBackend<Stdout>>;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = db_path()?;
    println!("using database: {}", db_path.display());

    Connection::file(&db_path.to_string_lossy())?;
    ensure_schema()?;

    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("tui") => run_tui()?,
        Some("list") => list_todos()?,
        Some("seed") => seed_demo_data()?,
        Some("add") => {
            let title = args.collect::<Vec<_>>().join(" ");
            if title.trim().is_empty() {
                return Err(io::Error::other("usage: cargo run --example todo -- add <title>").into());
            }
            add_todo(&title)?;
            list_todos()?;
        }
        Some("done") => {
            let id = args
                .next()
                .ok_or_else(|| io::Error::other("usage: cargo run --example todo -- done <id>"))?
                .parse::<u64>()?;
            mark_done(id)?;
            list_todos()?;
        }
        Some("clear-done") => {
            clear_done()?;
            list_todos()?;
        }
        Some("stats") => print_stats()?,
        Some("help") | Some("--help") | Some("-h") => print_help(),
        Some(other) => {
            return Err(io::Error::other(format!(
                "unknown command `{other}`. run `cargo run --example todo -- help`"
            ))
            .into());
        }
    }

    Ok(())
}

fn db_path() -> Result<PathBuf, io::Error> {
    let dir = PathBuf::from("target/examples");
    fs::create_dir_all(&dir)?;
    Ok(dir.join("todo.sqlite"))
}

fn ensure_schema() -> Result<(), Box<dyn std::error::Error>> {
    let plan = SchemaBuilder::registered()?.plan()?;

    if !plan.ops.is_empty() {
        println!("schema plan:");
        print_plan(&plan);
    }

    if plan.is_blocked() {
        eprintln!("schema plan is blocked:\n{}", plan.to_json_string());
        return Err(io::Error::other("schema plan is blocked; inspect the JSON above").into());
    }

    if plan.is_destructive() {
        eprintln!("refusing to auto-apply destructive schema changes in the example app");
        eprintln!("review plan JSON:\n{}", plan.to_json_string());
        return Err(io::Error::other(
            "destructive schema plan detected; this example only auto-applies safe changes",
        )
        .into());
    }

    if !plan.ops.is_empty() {
        plan.apply(ApplyMode::SafeOnly)?;
        println!("applied safe schema changes\n");
    }

    Ok(())
}

fn print_plan(plan: &Plan) {
    for op in &plan.ops {
        match op {
            PlanOp::CreateTable { table } => println!("  - create table `{}`", table.name),
            PlanOp::AddColumn { table, column } => {
                let nullable = if column.nullable { "nullable" } else { "not null" };
                println!(
                    "  - add column `{}.{}` {} {}",
                    table, column.name, column.sql_type, nullable
                );
            }
            PlanOp::RebuildTable { table, reasons, .. } => {
                println!("  - rebuild table `{}` ({})", table, reasons.len());
            }
            PlanOp::DropTable { table } => println!("  - drop table `{}`", table.name),
        }
    }
}

fn seed_demo_data() -> Result<(), Box<dyn std::error::Error>> {
    if Todo::exists()? {
        println!("database already has todos; skipping seed");
        return Ok(());
    }

    Todo::builder()
        .title("buy groceries")
        .done(false)
        .notes(Some("milk, onions, coffee".to_string()))
        .create()?;
    Todo::builder()
        .title("write release notes")
        .done(false)
        .notes(Some("mention schema planning work".to_string()))
        .create()?;
    Todo::builder()
        .title("take out recycling")
        .done(true)
        .create()?;

    println!("seeded demo todos");
    list_todos()?;
    Ok(())
}

fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    let mut terminal = setup_terminal()?;
    let result = run_tui_loop(&mut terminal);
    restore_terminal(&mut terminal)?;
    result
}

fn run_tui_loop(terminal: &mut TodoTerminal) -> Result<(), Box<dyn std::error::Error>> {
    let mut app = TodoApp::load()?;

    loop {
        terminal.draw(|frame| draw_tui(frame, &app))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }

        let Event::Key(key) = event::read()? else {
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }

        match app.mode {
            InputMode::Normal => match key.code {
                KeyCode::Char('q') => break,
                KeyCode::Down | KeyCode::Char('j') => app.next(),
                KeyCode::Up | KeyCode::Char('k') => app.previous(),
                KeyCode::Char('a') => {
                    app.mode = InputMode::Editing;
                    app.input.clear();
                    app.status = "add a todo title, then press Enter".into();
                }
                KeyCode::Char(' ') | KeyCode::Enter => {
                    if let Some(id) = app.selected_id() {
                        toggle_done(id)?;
                        app.refresh_with_status(format!("toggled todo #{id}"))?;
                    }
                }
                KeyCode::Char('d') => {
                    if let Some(id) = app.selected_id() {
                        mark_done(id)?;
                        app.refresh_with_status(format!("marked todo #{id} as done"))?;
                    }
                }
                KeyCode::Char('x') => {
                    if let Some((id, title)) = app.selected_summary() {
                        delete_todo(id)?;
                        app.refresh_with_status(format!("deleted todo #{id}: {title}"))?;
                    }
                }
                KeyCode::Char('c') => {
                    let removed = clear_done_count()?;
                    app.refresh_with_status(format!("cleared {removed} completed todo(s)"))?;
                }
                KeyCode::Char('s') => {
                    let seeded = seed_if_empty()?;
                    app.refresh_with_status(if seeded {
                        "seeded demo todos".into()
                    } else {
                        "database already had todos".into()
                    })?;
                }
                KeyCode::Char('r') => app.refresh()?,
                _ => {}
            },
            InputMode::Editing => match key.code {
                KeyCode::Esc => {
                    app.mode = InputMode::Normal;
                    app.input.clear();
                    app.status = "cancelled new todo".into();
                }
                KeyCode::Enter => {
                    let title = app.input.trim().to_string();
                    if title.is_empty() {
                        app.status = "todo title cannot be empty".into();
                    } else {
                        add_todo(&title)?;
                        app.mode = InputMode::Normal;
                        app.input.clear();
                        app.refresh_with_status(format!("added todo: {title}"))?;
                    }
                }
                KeyCode::Backspace => {
                    app.input.pop();
                }
                KeyCode::Char(ch) => app.input.push(ch),
                _ => {}
            },
        }
    }

    Ok(())
}

fn draw_tui(frame: &mut Frame<'_>, app: &TodoApp) {
    let layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(8), Constraint::Length(7)])
        .split(frame.area());

    let header = Paragraph::new(Text::from(vec![
        Line::from("seekwel todo example"),
        Line::from(format!(
            "total={} open={} done={}",
            app.todos.len(),
            app.todos.iter().filter(|todo| !todo.done).count(),
            app.todos.iter().filter(|todo| todo.done).count(),
        )),
    ]))
    .block(Block::default().borders(Borders::ALL).title("todos"));
    frame.render_widget(header, layout[0]);

    let body = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(layout[1]);

    let items = if app.todos.is_empty() {
        vec![ListItem::new("no todos yet — press 'a' to add or 's' to seed")]
    } else {
        app.todos
            .iter()
            .map(|todo| {
                let status = if todo.done { "[x]" } else { "[ ]" };
                ListItem::new(format!("{} #{:<3} {}", status, todo.id, todo.title))
            })
            .collect()
    };

    let list = List::new(items)
        .block(Block::default().borders(Borders::ALL).title("list"))
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    let mut state = ListState::default();
    state.select(app.selected_index());
    frame.render_stateful_widget(list, body[0], &mut state);

    let details = if let Some(todo) = app.selected_todo() {
        let notes = todo.notes.as_deref().unwrap_or("(no notes)");
        Paragraph::new(Text::from(vec![
            Line::from(format!("id: {}", todo.id)),
            Line::from(format!("title: {}", todo.title)),
            Line::from(format!("done: {}", todo.done)),
            Line::from(""),
            Line::from("notes:"),
            Line::from(notes),
        ]))
    } else {
        Paragraph::new("nothing selected")
    }
    .block(Block::default().borders(Borders::ALL).title("details"))
    .wrap(Wrap { trim: false });
    frame.render_widget(details, body[1]);

    let recent_queries = Connection::recent_queries();
    let mut query_lines = recent_queries
        .iter()
        .rev()
        .take((layout[2].height.saturating_sub(2)) as usize)
        .rev()
        .map(|query| Line::from(query.clone()))
        .collect::<Vec<_>>();
    if query_lines.is_empty() {
        query_lines.push(Line::from("(no queries yet)"));
    }
    if matches!(app.mode, InputMode::Normal) {
        query_lines.insert(0, Line::from(app.status.as_str()));
        query_lines.insert(
            1,
            Line::from("j/k or ↑/↓ move • Enter/Space toggle • a add • d done • x delete • c clear • s seed • r refresh • q quit"),
        );
        query_lines.insert(2, Line::from(""));
    }

    let queries = Paragraph::new(Text::from(query_lines))
        .block(Block::default().borders(Borders::ALL).title("queries"))
        .wrap(Wrap { trim: false });
    frame.render_widget(queries, layout[2]);

    if matches!(app.mode, InputMode::Editing) {
        let popup = centered_rect(80, 20, frame.area());
        frame.render_widget(Clear, popup);
        let input = Paragraph::new(app.input.as_str())
            .block(Block::default().borders(Borders::ALL).title("new todo"));
        frame.render_widget(input, popup);
        frame.set_cursor_position((popup.x + app.input.chars().count() as u16 + 1, popup.y + 1));
    }
}

fn setup_terminal() -> Result<TodoTerminal, Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    Ok(Terminal::new(backend)?)
}

fn restore_terminal(terminal: &mut TodoTerminal) -> Result<(), Box<dyn std::error::Error>> {
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}

fn centered_rect(percent_x: u16, percent_y: u16, area: ratatui::layout::Rect) -> ratatui::layout::Rect {
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(area);
    let horizontal = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vertical[1]);
    horizontal[1]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InputMode {
    Normal,
    Editing,
}

struct TodoApp {
    todos: Vec<Todo>,
    selected: usize,
    status: String,
    input: String,
    mode: InputMode,
}

impl TodoApp {
    fn load() -> Result<Self, Box<dyn std::error::Error>> {
        let todos = fetch_todos()?;
        Ok(Self {
            selected: 0,
            status: "ready".into(),
            input: String::new(),
            mode: InputMode::Normal,
            todos,
        })
    }

    fn refresh(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let previous_id = self.selected_todo().map(|todo| todo.id);
        self.todos = fetch_todos()?;
        self.selected = match previous_id {
            Some(id) => self
                .todos
                .iter()
                .position(|todo| todo.id == id)
                .unwrap_or_else(|| self.todos.len().saturating_sub(1)),
            None => 0,
        };
        Ok(())
    }

    fn refresh_with_status(&mut self, status: String) -> Result<(), Box<dyn std::error::Error>> {
        self.status = status;
        self.refresh()
    }

    fn next(&mut self) {
        if self.todos.is_empty() {
            self.selected = 0;
        } else {
            self.selected = (self.selected + 1) % self.todos.len();
        }
    }

    fn previous(&mut self) {
        if self.todos.is_empty() {
            self.selected = 0;
        } else if self.selected == 0 {
            self.selected = self.todos.len() - 1;
        } else {
            self.selected -= 1;
        }
    }

    fn selected_index(&self) -> Option<usize> {
        if self.todos.is_empty() {
            None
        } else {
            Some(self.selected.min(self.todos.len() - 1))
        }
    }

    fn selected_todo(&self) -> Option<&Todo> {
        self.selected_index().and_then(|index| self.todos.get(index))
    }

    fn selected_id(&self) -> Option<u64> {
        self.selected_todo().map(|todo| todo.id)
    }

    fn selected_summary(&self) -> Option<(u64, String)> {
        self.selected_todo()
            .map(|todo| (todo.id, todo.title.clone()))
    }
}

fn fetch_todos() -> Result<Vec<Todo>, Box<dyn std::error::Error>> {
    Ok(Todo::order(TodoColumns::Id).all()?)
}

fn seed_if_empty() -> Result<bool, Box<dyn std::error::Error>> {
    if Todo::exists()? {
        return Ok(false);
    }
    seed_demo_data()?;
    Ok(true)
}

fn add_todo(title: &str) -> Result<(), Box<dyn std::error::Error>> {
    add_todo_with_notes(title, None)
}

fn add_todo_with_notes(
    title: &str,
    notes: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    let todo = Todo::builder()
        .title(title.to_string())
        .done(false)
        .notes(notes.map(ToOwned::to_owned))
        .create()?;
    println!("added todo #{}: {}", todo.id, todo.title);
    Ok(())
}

fn mark_done(id: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut todo = Todo::find(id)?;
    todo.done = true;
    todo.save()?;
    Ok(())
}

fn toggle_done(id: u64) -> Result<(), Box<dyn std::error::Error>> {
    let mut todo = Todo::find(id)?;
    todo.done = !todo.done;
    todo.save()?;
    Ok(())
}

fn delete_todo(id: u64) -> Result<(), Box<dyn std::error::Error>> {
    let todo = Todo::find(id)?;
    todo.delete()?;
    Ok(())
}

fn clear_done() -> Result<(), Box<dyn std::error::Error>> {
    let _ = clear_done_count()?;
    Ok(())
}

fn clear_done_count() -> Result<usize, Box<dyn std::error::Error>> {
    let done = Todo::q(TodoColumns::Done, Comparison::Eq(true)).all()?;
    let count = done.len();
    for todo in done {
        todo.delete()?;
    }
    Ok(count)
}

fn list_todos() -> Result<(), Box<dyn std::error::Error>> {
    let todos = Todo::order(TodoColumns::Id).all()?;
    if todos.is_empty() {
        println!("no todos yet");
        println!("try: cargo run --example todo -- seed");
        return Ok(());
    }

    println!("todos:");
    for todo in todos {
        let status = if todo.done { "[x]" } else { "[ ]" };
        match todo.notes.as_deref() {
            Some(notes) => println!("  {} #{} {} — {}", status, todo.id, todo.title, notes),
            None => println!("  {} #{} {}", status, todo.id, todo.title),
        }
    }
    println!();
    print_stats()?;
    Ok(())
}

fn print_stats() -> Result<(), Box<dyn std::error::Error>> {
    let total = Todo::count()?;
    let open = Todo::q(TodoColumns::Done, Comparison::Eq(false)).count()?;
    let done = Todo::q(TodoColumns::Done, Comparison::Eq(true)).count()?;
    println!("stats: total={total}, open={open}, done={done}");
    Ok(())
}

fn print_help() {
    println!("todo example commands:");
    println!("  cargo run --example todo");
    println!("  cargo run --example todo -- tui");
    println!("  cargo run --example todo -- list");
    println!("  cargo run --example todo -- seed");
    println!("  cargo run --example todo -- add <title>");
    println!("  cargo run --example todo -- done <id>");
    println!("  cargo run --example todo -- clear-done");
    println!("  cargo run --example todo -- stats");
}
