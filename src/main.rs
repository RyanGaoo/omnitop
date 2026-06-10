mod app;
mod config;
mod docker;
mod gpu;
mod net;
mod ui;

use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};

use app::{App, InputMode, KillSignal, SortKey, View};
use config::Config;

fn main() -> std::io::Result<()> {
    let config = Config::load();
    let docker_rx = docker::spawn_poller(Duration::from_secs(2));
    let net_rx = net::spawn_poller(Duration::from_secs(2));
    let (action_tx, action_rx) = std::sync::mpsc::channel::<String>();
    let mut terminal = ratatui::init();
    let mut app = App::new(&config);
    let tick_rate = config.refresh;
    let mut last_tick = Instant::now();

    loop {
        while let Ok(state) = docker_rx.try_recv() {
            app.set_docker(state);
        }
        while let Ok(rates) = net_rx.try_recv() {
            app.set_net_rates(rates);
        }
        while let Ok(msg) = action_rx.try_recv() {
            app.status = Some(msg);
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
                            KeyCode::Char('N') => app.sort_by(SortKey::Net),
                            KeyCode::Char('s') if app.view == View::Containers => {
                                dispatch_container_action(
                                    &mut app,
                                    docker::ContainerAction::Stop,
                                    &action_tx,
                                );
                            }
                            KeyCode::Char('r') if app.view == View::Containers => {
                                dispatch_container_action(
                                    &mut app,
                                    docker::ContainerAction::Restart,
                                    &action_tx,
                                );
                            }
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

/// Run a container action (stop/restart) on a background thread so the UI never blocks
/// on the call (a stop can take ~10s). The result is reported back via the channel.
fn dispatch_container_action(
    app: &mut App,
    action: docker::ContainerAction,
    tx: &std::sync::mpsc::Sender<String>,
) {
    let Some((id, name)) = app
        .selected_container()
        .map(|c| (c.id.clone(), c.name.clone()))
    else {
        return;
    };
    app.status = Some(format!("{} {name}…", action.gerund()));
    let tx = tx.clone();
    std::thread::spawn(move || {
        let msg = match docker::run_action(action, &id) {
            Ok(()) => format!("{name} {}", action.past()),
            Err(e) => format!("{name}: {e}"),
        };
        let _ = tx.send(msg);
    });
}
