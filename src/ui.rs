use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Sparkline, Table};
use ratatui::Frame;

use crate::app::{App, InputMode, SortKey};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_header(frame, app, chunks[0]);
    draw_process_table(frame, app, chunks[1]);
    draw_footer(frame, app, chunks[2]);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let halves = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(area);

    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" CPU ({} cores) ", app.per_core.len()));
    let cpu_inner = cpu_block.inner(halves[0]);
    frame.render_widget(cpu_block, halves[0]);

    let cpu_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(cpu_inner);

    let cpu_gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color(app.cpu_usage)))
        .ratio((app.cpu_usage as f64 / 100.0).clamp(0.0, 1.0))
        .label(format!("{:.1}%", app.cpu_usage));
    frame.render_widget(cpu_gauge, cpu_rows[0]);

    frame.render_widget(per_core_line(&app.per_core), cpu_rows[1]);

    let cpu_data: Vec<u64> = app.cpu_history.iter().copied().collect();
    let cpu_spark = Sparkline::default()
        .data(&cpu_data)
        .max(100)
        .style(Style::default().fg(Color::Cyan));
    frame.render_widget(cpu_spark, cpu_rows[2]);

    let mem_block = Block::default().borders(Borders::ALL).title(" Memory ");
    let mem_inner = mem_block.inner(halves[1]);
    frame.render_widget(mem_block, halves[1]);

    let mem_rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(3)])
        .split(mem_inner);

    let mem_pct = if app.mem_total > 0 {
        app.mem_used as f64 / app.mem_total as f64
    } else {
        0.0
    };
    let mem_gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color((mem_pct * 100.0) as f32)))
        .ratio(mem_pct.clamp(0.0, 1.0))
        .label(format!(
            "{} / {}",
            format_bytes(app.mem_used),
            format_bytes(app.mem_total)
        ));
    frame.render_widget(mem_gauge, mem_rows[0]);

    let mem_data: Vec<u64> = app.mem_history.iter().copied().collect();
    let mem_spark = Sparkline::default()
        .data(&mem_data)
        .max(100)
        .style(Style::default().fg(Color::Magenta));
    frame.render_widget(mem_spark, mem_rows[1]);
}

fn per_core_line(per_core: &[f32]) -> Paragraph<'static> {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let spans: Vec<Span> = per_core
        .iter()
        .map(|&usage| {
            let idx = ((usage / 100.0 * 7.0).round() as usize).min(7);
            Span::styled(
                LEVELS[idx].to_string(),
                Style::default().fg(gauge_color(usage)),
            )
        })
        .collect();
    Paragraph::new(Line::from(spans))
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
        .visible
        .iter()
        .filter_map(|&idx| app.processes.get(idx))
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
            .title(if app.filter.is_empty() {
                format!(" Processes ({}) ", app.processes.len())
            } else {
                format!(
                    " Processes ({}/{}) — filter: {} ",
                    app.visible.len(),
                    app.processes.len(),
                    app.filter
                )
            }),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_footer(frame: &mut Frame, app: &App, area: Rect) {
    let line = match app.input_mode {
        InputMode::Filter => Line::from(vec![
            " /".bold().cyan(),
            app.filter.clone().into(),
            "█".into(),
            "  (Enter to apply, Esc to clear)".dark_gray(),
        ]),
        InputMode::ConfirmKill => {
            let target = app
                .selected_proc()
                .map(|p| format!("{} ({})", p.name, p.pid))
                .unwrap_or_else(|| "?".to_string());
            Line::from(vec![
                format!(" Send {} to {target}? ", app.pending_signal.label())
                    .bold()
                    .red(),
                "y".bold().cyan(),
                " to confirm, any other key to cancel".into(),
            ])
        }
        InputMode::Normal => {
            if let Some(status) = &app.status {
                Line::from(vec![" ".into(), status.clone().yellow()])
            } else {
                Line::from(vec![
                    " q ".bold().cyan(),
                    "quit  ".into(),
                    "↑/↓ ".bold().cyan(),
                    "navigate  ".into(),
                    "/ ".bold().cyan(),
                    "filter  ".into(),
                    "x/X ".bold().cyan(),
                    "term/kill  ".into(),
                    "space ".bold().cyan(),
                    "pause  ".into(),
                    "c/m/p/n ".bold().cyan(),
                    "sort".into(),
                ])
            }
        }
    };
    frame.render_widget(Paragraph::new(line), area);
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
