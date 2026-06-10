use ratatui::layout::{Alignment, Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Gauge, Paragraph, Row, Sparkline, Table, Wrap};
use ratatui::Frame;

use crate::app::{App, InputMode, SortKey, View};

pub fn draw(frame: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(6),
            Constraint::Min(5),
            Constraint::Length(1),
        ])
        .split(frame.area());

    draw_tabs(frame, app, chunks[0]);
    draw_header(frame, app, chunks[1]);
    match app.view {
        View::Processes => draw_process_table(frame, app, chunks[2]),
        View::Containers => draw_container_view(frame, app, chunks[2]),
    }
    draw_footer(frame, app, chunks[3]);
}

fn draw_tabs(frame: &mut Frame, app: &App, area: Rect) {
    let tab = |label: &str, active: bool| -> Span {
        if active {
            Span::styled(
                format!(" {label} "),
                Style::default()
                    .fg(Color::Black)
                    .bg(app.accent)
                    .add_modifier(Modifier::BOLD),
            )
        } else {
            Span::styled(format!(" {label} "), Style::default().fg(Color::Gray))
        }
    };

    let container_label = if app.containers.is_empty() {
        "2 Containers".to_string()
    } else {
        format!("2 Containers ({})", app.containers.len())
    };

    let line = Line::from(vec![
        Span::styled(" omnitop ", Style::default().add_modifier(Modifier::BOLD)),
        tab("1 Processes", app.view == View::Processes),
        Span::raw(" "),
        tab(&container_label, app.view == View::Containers),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn draw_header(frame: &mut Frame, app: &App, area: Rect) {
    let has_gpu = !app.gpus.is_empty();
    let constraints: Vec<Constraint> = if has_gpu {
        vec![
            Constraint::Percentage(40),
            Constraint::Percentage(35),
            Constraint::Percentage(25),
        ]
    } else {
        vec![Constraint::Percentage(50), Constraint::Percentage(50)]
    };
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(constraints)
        .split(area);

    let cpu_block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" CPU ({} cores) ", app.per_core.len()));
    let cpu_inner = cpu_block.inner(cols[0]);
    frame.render_widget(cpu_block, cols[0]);

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
        .style(Style::default().fg(app.accent));
    frame.render_widget(cpu_spark, cpu_rows[2]);

    let mem_block = Block::default().borders(Borders::ALL).title(" Memory ");
    let mem_inner = mem_block.inner(cols[1]);
    frame.render_widget(mem_block, cols[1]);

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

    if has_gpu {
        draw_gpu_panel(frame, app, cols[2]);
    }
}

fn draw_gpu_panel(frame: &mut Frame, app: &App, area: Rect) {
    let Some(gpu) = app.gpus.first() else {
        return;
    };
    let title = match gpu.name.strip_prefix("AGXAccelerator") {
        Some(model) if !model.is_empty() => format!(" GPU: Apple {model} "),
        _ => " GPU ".to_string(),
    };
    let block = Block::default().borders(Borders::ALL).title(title);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .split(inner);

    let util = gpu.utilization.unwrap_or(0.0);
    let gauge = Gauge::default()
        .gauge_style(Style::default().fg(gauge_color(util)))
        .ratio((util as f64 / 100.0).clamp(0.0, 1.0))
        .label(format!("{util:.0}%"));
    frame.render_widget(gauge, rows[0]);

    let mem_line = match (gpu.mem_used, gpu.mem_total) {
        (Some(u), Some(t)) => format!("mem {} / {}", format_bytes(u), format_bytes(t)),
        (Some(u), None) => format!("mem {}", format_bytes(u)),
        _ => "mem n/a".to_string(),
    };
    frame.render_widget(
        Paragraph::new(mem_line).style(Style::default().fg(Color::Gray)),
        rows[1],
    );

    let gpu_data: Vec<u64> = app.gpu_history.iter().copied().collect();
    let gpu_spark = Sparkline::default()
        .data(&gpu_data)
        .max(100)
        .style(Style::default().fg(Color::Green));
    frame.render_widget(gpu_spark, rows[2]);
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
    let accent = app.accent;
    let sort_label = |key: SortKey, label: &str| -> Span {
        if app.sort_key == key {
            Span::styled(
                format!("{label} ▼"),
                Style::default().add_modifier(Modifier::BOLD).fg(accent),
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
        Line::from("RX/s"),
        Line::from("TX/s"),
    ])
    .style(Style::default().add_modifier(Modifier::BOLD))
    .bottom_margin(1);

    let tree = app.tree && app.filter.is_empty();
    let rows: Vec<Row> = app
        .visible
        .iter()
        .enumerate()
        .filter_map(|(row, &idx)| app.processes.get(idx).map(|p| (row, p)))
        .map(|(row, p)| {
            let name = if tree {
                let depth = app.depths.get(row).copied().unwrap_or(0) as usize;
                if depth == 0 {
                    p.name.clone()
                } else {
                    format!("{}└─ {}", "  ".repeat(depth - 1), p.name)
                }
            } else {
                p.name.clone()
            };
            let rate = app.net_rates.get(&p.pid).copied().unwrap_or_default();
            Row::new(vec![
                p.pid.to_string(),
                name,
                format!("{:.1}", p.cpu),
                format_bytes(p.mem_bytes),
                fmt_rate(rate.rx_bps),
                fmt_rate(rate.tx_bps),
            ])
        })
        .collect();

    let title = if !app.filter.is_empty() {
        format!(
            " Processes ({}/{}) — filter: {} ",
            app.visible.len(),
            app.processes.len(),
            app.filter
        )
    } else if tree {
        format!(" Processes ({}) — tree ", app.processes.len())
    } else {
        format!(" Processes ({}) ", app.processes.len())
    };
    let mut block = Block::default().borders(Borders::ALL).title(title);
    let (rx_total, tx_total) = app.net_totals();
    if rx_total > 0 || tx_total > 0 {
        block = block.title_bottom(
            Line::from(format!(
                " net ↓{}/s  ↑{}/s ",
                format_bytes(rx_total),
                format_bytes(tx_total)
            ))
            .right_aligned(),
        );
    }

    let table = Table::new(
        rows,
        [
            Constraint::Length(8),
            Constraint::Min(20),
            Constraint::Length(8),
            Constraint::Length(12),
            Constraint::Length(11),
            Constraint::Length(11),
        ],
    )
    .header(header)
    .block(block)
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(table, area, &mut app.table_state);
}

fn draw_container_view(frame: &mut Frame, app: &mut App, area: Rect) {
    if let Some(msg) = &app.docker_message {
        let block = Block::default().borders(Borders::ALL).title(" Containers ");
        let paragraph = Paragraph::new(msg.clone())
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true })
            .style(Style::default().fg(Color::Gray));
        frame.render_widget(paragraph, area);
        return;
    }

    let header = Row::new(vec!["ID", "NAME", "IMAGE", "STATUS", "CPU%", "MEM"])
        .style(Style::default().add_modifier(Modifier::BOLD))
        .bottom_margin(1);

    let rows: Vec<Row> = app
        .containers
        .iter()
        .map(|c| {
            let mem = if c.mem_limit > 0 {
                format!(
                    "{} / {}",
                    format_bytes(c.mem_used),
                    format_bytes(c.mem_limit)
                )
            } else {
                format_bytes(c.mem_used)
            };
            Row::new(vec![
                c.id.clone(),
                truncate(&c.name, 24),
                truncate(&c.image, 28),
                truncate(&c.status, 18),
                format!("{:.1}", c.cpu),
                mem,
            ])
        })
        .collect();

    let table = Table::new(
        rows,
        [
            Constraint::Length(13),
            Constraint::Min(16),
            Constraint::Length(30),
            Constraint::Length(20),
            Constraint::Length(8),
            Constraint::Length(22),
        ],
    )
    .header(header)
    .block(
        Block::default()
            .borders(Borders::ALL)
            .title(format!(" Containers ({}) ", app.containers.len())),
    )
    .row_highlight_style(Style::default().bg(Color::DarkGray).add_modifier(Modifier::BOLD));

    frame.render_stateful_widget(table, area, &mut app.container_state);
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let kept: String = s.chars().take(max.saturating_sub(1)).collect();
        format!("{kept}…")
    }
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
                match app.view {
                    View::Processes => Line::from(vec![
                        " q ".bold().cyan(),
                        "quit  ".into(),
                        "Tab ".bold().cyan(),
                        "view  ".into(),
                        "↑/↓ ".bold().cyan(),
                        "navigate  ".into(),
                        "/ ".bold().cyan(),
                        "filter  ".into(),
                        "x/X ".bold().cyan(),
                        "term/kill  ".into(),
                        "t ".bold().cyan(),
                        "tree  ".into(),
                        "c/m/p/n ".bold().cyan(),
                        "sort".into(),
                    ]),
                    View::Containers => Line::from(vec![
                        " q ".bold().cyan(),
                        "quit  ".into(),
                        "Tab ".bold().cyan(),
                        "view  ".into(),
                        "↑/↓ ".bold().cyan(),
                        "navigate  ".into(),
                        "refresh every 2s".dark_gray(),
                    ]),
                }
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

/// Format a per-second byte rate for the process table; idle processes show a dash.
fn fmt_rate(bps: u64) -> String {
    if bps == 0 {
        "-".to_string()
    } else {
        format!("{}/s", format_bytes(bps))
    }
}
