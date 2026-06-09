use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Table};
use ratatui::Frame;

use crate::app::{App, SortKey};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(4),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);
    draw_process_table(frame, app, chunks[1]);
    draw_footer(frame, chunks[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let cpu_gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" CPU ({} cores) ", app.per_core.len())),
        )
        .gauge_style(Style::default().fg(gauge_color(app.cpu_usage)))
        .ratio((app.cpu_usage as f64 / 100.0).clamp(0.0, 1.0))
        .label(format!("{:.1}%", app.cpu_usage));
    frame.render_widget(cpu_gauge, halves[0]);

    let mem_pct = if app.mem_total > 0 {
        app.mem_used as f64 / app.mem_total as f64
    } else {
        0.0
    };
    let mem_gauge = Gauge::default()
        .block(Block::default().borders(Borders::ALL).title(" Memory "))
        .gauge_style(Style::default().fg(gauge_color((mem_pct * 100.0) as f32)))
        .ratio(mem_pct.clamp(0.0, 1.0))
        .label(format!(
            "{} / {}",
            format_bytes(app.mem_used),
            format_bytes(app.mem_total)
        ));
    frame.render_widget(mem_gauge, halves[1]);
}

fn draw_process_table(frame: &mut Frame, app: &mut App, area: Rect) {
    let sort_label = |key: SortKey, label: &str| -> Span {
        if app.sort_key == key {
            Span::styled(
                format!("{label} ▼"),
                Style::default().add_modifier(Modifier::BOLD).fg(Color::Cyan),
            )
        } else {
            Span::raw(label.to_string())
        }
    };

    let header = Row::new(vec![
        Line::from(sort_label(SortKey::Pid, "PID")),
        Line::from(sort_label(SortKey::Name, "NAME")),
        Line::from(sort_label(SortKey::Cpu, "CPU%")),
        Line::from(sort_label(SortKey::Memory, "MEM")),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let rows: Vec<Row> = app
        .processes
        .iter()
        .map(|p| {
            Row::new(vec![
                p.pid.to_string(),
                p.name.clone(),
                format!("{:.1}", p.cpu),
                format_bytes(p.mem_bytes),
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(12),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Processes ({}) ", app.processes.len())),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_footer(frame: &mut Frame, area: Rect) {
    let help = Paragraph::new(Line::from(vec![
        " q ".bold().cyan(),
        "quit  ".into(),
        "↑/↓ j/k ".bold().cyan(),
        "navigate  ".into(),
        "c ".bold().cyan(),
        "sort cpu  ".into(),
        "m ".bold().cyan(),
        "sort mem  ".into(),
        "p ".bold().cyan(),
        "sort pid  ".into(),
        "n ".bold().cyan(),
        "sort name".into(),
    ]));
    frame.render_widget(help, area);
}

fn gauge_color(pct: f32) -> Color {
    if pct > 85.0 {
        Color::Red
    } else if pct > 60.0 {
        Color::Yellow
    } else {
        Color::Green
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}
