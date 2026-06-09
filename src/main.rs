mod app;
mod config;
mod docker;
mod gpu;
mod ui;

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use app::{App, InputMode, KillSignal, SortKey, View};
use config::Config;

fn main() -> std::io::Result<()> {
    let config = Config::load();
    let docker_rx = docker::spawn_poller(Duration::from_secs(2));
    let mut terminal = ratatui::init();
    let mut app = App::new(&config);
    let tick_rate = config.refresh;
    let mut last_tick = Instant::now();

    loop {
        while let Ok(state) = docker_rx.try_recv() {
            app.set_docker(state);
        }

        terminal.draw(|frame| ui::draw(frame, &mut app))?;

        let timeout = tick_rate.saturating_sub(last_tick.elapsed());
        if event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.input_mode {
                        InputMode::Filter => match key.code {
                            KeyCode::Esc => {
                                app.filter.clear();
                                app.input_mode = InputMode::Normal;
                                app.apply_filter();
                            }
                            KeyCode::Enter => app.input_mode = InputMode::Normal,
                            KeyCode::Backspace => {
                                app.filter.pop();
                                app.apply_filter();
                            }
                            KeyCode::Char(ch) => {
                                app.filter.push(ch);
                                app.apply_filter();
                            }
                            _ => {}
                        },
                        InputMode::ConfirmKill => {
                            if matches!(key.code, KeyCode::Char('y') | KeyCode::Char('Y')) {
                                app.kill_selected();
                            }
                            app.input_mode = InputMode::Normal;
                        }
                        InputMode::Normal => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => break,
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.next(),
                            KeyCode::Char('/') => {
                                app.status = None;
                                app.input_mode = InputMode::Filter;
                            }
                            KeyCode::Tab => app.toggle_view(),
                            KeyCode::Char('1') => app.set_view(View::Processes),
                            KeyCode::Char('2') => app.set_view(View::Containers),
                            KeyCode::Char('x') => {
                                if app.view == View::Processes && app.selected_proc().is_some() {
                                    app.pending_signal = KillSignal::Term;
                                    app.input_mode = InputMode::ConfirmKill;
                                }
                            }
                            KeyCode::Char('X') => {
                                if app.view == View::Processes && app.selected_proc().is_some() {
                                    app.pending_signal = KillSignal::Kill;
                                    app.input_mode = InputMode::ConfirmKill;
                                }
                            }
                            KeyCode::Char(' ') => app.toggle_pause(),
                            KeyCode::Char('t') => app.toggle_tree(),
                            KeyCode::Char('c') => app.sort_by(SortKey::Cpu),
                            KeyCode::Char('m') => app.sort_by(SortKey::Memory),
                            KeyCode::Char('p') => app.sort_by(SortKey::Pid),
                            KeyCode::Char('n') => app.sort_by(SortKey::Name),
                            _ => {}
                        },
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            if !app.paused {
                app.refresh();
            }
            last_tick = Instant::now();
        }
    }

    ratatui::restore();
    Ok(())
}
