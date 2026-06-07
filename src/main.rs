mod i18n;

use anyhow::Result;
use clap::Parser;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Gauge, Paragraph, Row, Table, Wrap},
    Terminal,
};
use std::io;
use std::time::Duration;
use sysinfo::System;
use tokio::time::interval;
use tracing::{error, info, warn};

use i18n::{t, td, DynKey, Key};

/// ra-killer: Auto-monitor and kill high-memory rust-analyzer processes
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Memory usage threshold (percent, 0-100)
    #[arg(short, long, default_value_t = 85, value_parser = clap::value_parser!(u8).range(1..100))]
    threshold: u8,

    /// Check interval (seconds)
    #[arg(short, long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(5..3600))]
    interval: u64,

    /// Target process name
    #[arg(short, long, default_value = "rust-analyzer")]
    process: String,

    /// Run check once
    #[arg(long)]
    once: bool,

    /// Enable TUI mode
    #[arg(long)]
    tui: bool,

    /// Verbose logging
    #[arg(long)]
    verbose: bool,

    /// Language (zh/en/auto)
    #[arg(long, default_value = "auto")]
    lang: String,
}

#[derive(Clone, Copy)]
enum AppState {
    Normal,
    ConfirmKill(usize),
}

/// RAII guard to ensure terminal is always restored
struct TuiGuard;

impl Drop for TuiGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            io::stdout(),
            LeaveAlternateScreen,
            DisableMouseCapture
        );
    }
}

/// Run TUI interface
async fn run_tui(args: Args) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let _guard = TuiGuard;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(args.threshold, args.interval, args.process.clone());
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(250);

    while !app.should_quit {
        app.clear_expired_message();

        terminal.draw(|f| draw_ui(f, &app))?;

        if event::poll(tick_rate)? {
            if let Event::Key(key) = event::read()? {
                match app.state {
                    AppState::Normal => {
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => {
                                app.should_quit = true;
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                if app.selected_index < app.processes.len().saturating_sub(1) {
                                    app.selected_index += 1;
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                if app.selected_index > 0 {
                                    app.selected_index -= 1;
                                }
                            }
                            KeyCode::Enter => {
                                if !app.processes.is_empty() {
                                    app.state = AppState::ConfirmKill(app.selected_index);
                                }
                            }
                            KeyCode::Char('r') => {
                                app.refresh_processes();
                                app.set_message(t(Key::msg_refreshed).to_string());
                            }
                            KeyCode::Char('a') => {
                                if !app.processes.is_empty() {
                                    app.state = AppState::ConfirmKill(usize::MAX);
                                }
                            }
                            KeyCode::Char('L') => {
                                i18n::toggle();
                                app.set_message(t(Key::msg_lang_switched).to_string());
                            }
                            _ => {}
                        }
                    }
                    AppState::ConfirmKill(index) => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => {
                                if index == usize::MAX {
                                    match app.kill_top_memory_processes(40) {
                                        Ok(killed) => {
                                            app.set_message(td(DynKey::MsgKilledCount(killed)));
                                        }
                                        Err(e) => {
                                            app.set_message(td(DynKey::MsgKillFailed(e.to_string())));
                                        }
                                    }
                                } else {
                                    let _ = app.kill_selected_process();
                                }
                                app.state = AppState::Normal;
                            }
                            KeyCode::Char('n') | KeyCode::Esc => {
                                app.state = AppState::Normal;
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Auto refresh
        if last_tick.elapsed() >= Duration::from_secs(args.interval) {
            app.refresh_processes();
            if app.memory_percent() >= app.threshold as f64 {
                match app.auto_kill_high_memory() {
                    Ok(killed) if killed > 0 => {
                        app.set_message(td(DynKey::MsgAutoKilled(killed)));
                        app.refresh_processes();
                    }
                    _ => {}
                }
            }
            last_tick = std::time::Instant::now();
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Initialize i18n
    if args.lang != "auto" {
        i18n::init(Some(&args.lang));
    } else {
        i18n::init(None);
    }

    // Initialize logging
    let log_level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    if !args.tui {
        tracing_subscriber::fmt().with_max_level(log_level).init();
    }

    if args.tui {
        return run_tui(args).await;
    }

    // CLI mode
    info!("{}", t(Key::cli_starting));
    info!("{}", td(DynKey::CliThreshold(args.threshold)));
    info!("{}", td(DynKey::CliInterval(args.interval)));
    info!("{}", td(DynKey::CliTarget(args.process.clone())));

    let mut sys = System::new_all();

    if args.once {
        run_check(&mut sys, &args);
        return Ok(());
    }

    let mut timer = interval(Duration::from_secs(args.interval));

    loop {
        timer.tick().await;
        run_check(&mut sys, &args);
    }
}

fn run_check(sys: &mut System, args: &Args) {
    sys.refresh_memory();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let total_memory = sys.total_memory();
    let used_memory = sys.used_memory();
    let memory_usage_percent = if total_memory == 0 {
        0.0
    } else {
        used_memory as f64 / total_memory as f64 * 100.0
    };

    info!(
        "{}",
        td(DynKey::CliMemoryUsage {
            used: format_bytes(used_memory),
            total: format_bytes(total_memory),
            percent: memory_usage_percent,
        })
    );

    if memory_usage_percent >= args.threshold as f64 {
        warn!(
            "{}",
            td(DynKey::CliMemoryWarning {
                percent: memory_usage_percent,
                threshold: args.threshold,
            })
        );

        match find_and_kill_target_process(sys, &args.process) {
            Some(killed_count) => {
                if killed_count > 0 {
                    info!(
                        "{}",
                        td(DynKey::CliKilledReleased {
                            count: killed_count,
                            name: args.process.clone(),
                        })
                    );
                } else {
                    info!("{}", td(DynKey::CliNoProcess(args.process.clone())));
                }
            }
            None => {
                error!("{}", t(Key::cli_kill_error));
            }
        }
    } else {
        show_target_processes(sys, &args.process);
    }
}

fn find_matching_processes<'a>(
    sys: &'a System,
    target_name: &str,
) -> Vec<(sysinfo::Pid, &'a sysinfo::Process)> {
    sys.processes()
        .iter()
        .filter(|(_, process)| {
            process.name().to_string_lossy().contains(target_name)
        })
        .map(|(pid, process)| (*pid, process))
        .collect()
}

fn find_and_kill_target_process(sys: &System, target_name: &str) -> Option<usize> {
    let mut killed_count = 0;

    for (pid, process) in find_matching_processes(sys, target_name) {
        let memory_usage = process.memory();
        let pid_u32 = pid.as_u32();

        info!(
            "{}",
            td(DynKey::CliFoundDetail {
                name: target_name.to_string(),
                pid: pid_u32,
                mem: format_bytes(memory_usage),
            })
        );

        if let Err(e) = kill_process(pid_u32) {
            warn!(
                "{}",
                td(DynKey::CliCannotTerminate {
                    pid: pid_u32,
                    error: e.to_string(),
                })
            );
        } else {
            info!("{}", td(DynKey::CliTerminated(pid_u32)));
            killed_count += 1;
        }
    }

    Some(killed_count)
}

fn show_target_processes(sys: &System, target_name: &str) {
    let mut found = false;
    for (pid, process) in find_matching_processes(sys, target_name) {
        let memory_usage = process.memory();
        let pid_u32 = pid.as_u32();
        info!(
            "{}",
            td(DynKey::CliProcessDetail {
                name: target_name.to_string(),
                pid: pid_u32,
                mem: format_bytes(memory_usage),
            })
        );
        found = true;
    }

    if found {
        info!("{}", t(Key::cli_hint_once));
    }
}

fn kill_process(pid: u32) -> Result<()> {
    use std::process::Command;
    use std::thread;
    use std::time::Duration;

    let output = Command::new("kill")
        .arg("-15")
        .arg(pid.to_string())
        .output()?;

    if !output.status.success() {
        anyhow::bail!("{}: {:?}", t(Key::err_kill_failed), output.status);
    }

    thread::sleep(Duration::from_millis(500));

    let check = Command::new("kill")
        .arg("-0")
        .arg(pid.to_string())
        .output();

    if let Ok(output) = check {
        if output.status.success() {
            warn!("{}", td(DynKey::CliSigtermFailed(pid)));
            let kill_output = Command::new("kill")
                .arg("-9")
                .arg(pid.to_string())
                .output()?;
            if !kill_output.status.success() {
                anyhow::bail!("{}: {:?}", t(Key::err_sigkill_failed), kill_output.status);
            }
        }
    }

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;
    const KB: u64 = 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

struct ProcessInfo {
    pid: u32,
    name: String,
    memory: u64,
    cpu: f32,
}

struct App {
    sys: System,
    threshold: u8,
    interval: u64,
    process_name: String,
    selected_index: usize,
    processes: Vec<ProcessInfo>,
    state: AppState,
    should_quit: bool,
    last_refresh: std::time::Instant,
    message: Option<String>,
    message_time: Option<std::time::Instant>,
}

impl App {
    fn new(threshold: u8, interval: u64, process_name: String) -> Self {
        let mut sys = System::new_all();
        sys.refresh_memory();
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let mut app = Self {
            sys,
            threshold,
            interval,
            process_name,
            selected_index: 0,
            processes: Vec::new(),
            state: AppState::Normal,
            should_quit: false,
            last_refresh: std::time::Instant::now(),
            message: None,
            message_time: None,
        };
        app.refresh_processes();
        app
    }

    fn refresh_processes(&mut self) {
        self.sys.refresh_memory();
        self.sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

        self.processes.clear();

        for (pid, process) in find_matching_processes(&self.sys, &self.process_name) {
            self.processes.push(ProcessInfo {
                pid: pid.as_u32(),
                name: process.name().to_string_lossy().to_string(),
                memory: process.memory(),
                cpu: process.cpu_usage(),
            });
        }

        if self.selected_index >= self.processes.len() && !self.processes.is_empty() {
            self.selected_index = self.processes.len() - 1;
        }

        self.last_refresh = std::time::Instant::now();
    }

    fn total_memory(&self) -> u64 {
        self.sys.total_memory()
    }

    fn used_memory(&self) -> u64 {
        self.sys.used_memory()
    }

    fn memory_percent(&self) -> f64 {
        let total = self.total_memory();
        if total == 0 {
            0.0
        } else {
            (self.used_memory() as f64 / total as f64) * 100.0
        }
    }

    fn auto_kill_high_memory(&mut self) -> Result<usize> {
        self.kill_top_memory_processes(40)
    }

    fn kill_selected_process(&mut self) -> Result<()> {
        if self.selected_index < self.processes.len() {
            let pid = self.processes[self.selected_index].pid;
            kill_process(pid)?;
            self.set_message(td(DynKey::MsgKilledProcess(pid)));
            self.refresh_processes();
        }
        Ok(())
    }

    fn kill_top_memory_processes(&mut self, percentage: u8) -> Result<usize> {
        if self.processes.is_empty() {
            return Ok(0);
        }

        let mut sorted_processes: Vec<_> = self.processes.iter().enumerate().collect();
        sorted_processes.sort_by(|a, b| b.1.memory.cmp(&a.1.memory));

        let total = self.processes.len();
        let kill_count = (total as f64 * percentage as f64 / 100.0).ceil() as usize;

        let mut killed = 0;
        let mut killed_pids = std::collections::HashSet::new();

        for (_idx, proc_info) in sorted_processes.iter().take(kill_count) {
            if let Err(e) = kill_process(proc_info.pid) {
                warn!(
                    "{}",
                    td(DynKey::CliCannotTerminate {
                        pid: proc_info.pid,
                        error: e.to_string(),
                    })
                );
            } else {
                killed_pids.insert(proc_info.pid);
                killed += 1;
            }
        }

        self.processes.retain(|p| !killed_pids.contains(&p.pid));

        if self.selected_index >= self.processes.len() && !self.processes.is_empty() {
            self.selected_index = self.processes.len() - 1;
        }

        Ok(killed)
    }

    fn set_message(&mut self, msg: String) {
        self.message = Some(msg);
        self.message_time = Some(std::time::Instant::now());
    }

    fn clear_expired_message(&mut self) {
        if let Some(msg_time) = self.message_time {
            if msg_time.elapsed() >= std::time::Duration::from_secs(3) {
                self.message = None;
                self.message_time = None;
            }
        }
    }
}

fn draw_ui(f: &mut ratatui::Frame, app: &App) {
    let size = f.area();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(9),
                Constraint::Min(0),
                Constraint::Length(3),
            ]
            .as_ref(),
        )
        .split(size);

    // Title
    let title = Paragraph::new(vec![
        Line::from(vec![
            Span::styled(
                "🔫 ra-killer ",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("v{}", env!("CARGO_PKG_VERSION")),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                t(Key::target_process_label),
                Style::default().fg(Color::Cyan),
            ),
            Span::styled(
                &app.process_name,
                Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ),
        ]),
    ])
    .block(Block::default().borders(Borders::ALL))
    .alignment(Alignment::Center);
    f.render_widget(title, chunks[0]);

    // System info
    let memory_percent = app.memory_percent();
    let gauge_color = if memory_percent >= app.threshold as f64 {
        Color::Red
    } else if memory_percent >= app.threshold as f64 * 0.8 {
        Color::Yellow
    } else {
        Color::Green
    };

    let system_info = vec![
        Line::from(vec![
            Span::styled(t(Key::memory_usage_label), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{} / {}", format_bytes(app.used_memory()), format_bytes(app.total_memory())),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled(t(Key::usage_label), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{:.1}%", memory_percent),
                Style::default().fg(gauge_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled(t(Key::threshold_label), Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}%", app.threshold),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled(t(Key::refresh_interval_label), Style::default().fg(Color::Cyan)),
            Span::styled(
                td(DynKey::RefreshIntervalSecs(app.interval)),
                Style::default().fg(Color::Gray),
            ),
        ]),
        Line::from(vec![
            Span::styled(t(Key::auto_cleanup_label), Style::default().fg(Color::Cyan)),
            Span::styled(
                t(Key::auto_cleanup_enabled),
                Style::default().fg(Color::Green),
            ),
        ]),
    ];

    let gauge = Gauge::default()
        .block(Block::default().title(t(Key::system_status)).borders(Borders::ALL))
        .gauge_style(Style::default().fg(gauge_color).bg(Color::DarkGray))
        .percent(memory_percent.min(100.0) as u16)
        .label(format!("{:.1}%", memory_percent));

    let sys_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let sys_info = Paragraph::new(system_info)
        .block(Block::default().title(t(Key::system_info)).borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(sys_info, sys_chunks[0]);
    f.render_widget(gauge, sys_chunks[1]);

    // Process table
    let rows: Vec<Row> = app
        .processes
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let style = if i == app.selected_index {
                Style::default().fg(Color::Yellow).add_modifier(Modifier::REVERSED)
            } else {
                Style::default()
            };
            Row::new(vec![
                Cell::from(format!("{}", p.pid)),
                Cell::from(p.name.as_str()),
                Cell::from(format!("{:.1}%", p.cpu)),
                Cell::from(format_bytes(p.memory)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(8), Constraint::Min(10), Constraint::Length(8), Constraint::Min(0)])
        .header(Row::new(vec![
            Cell::from(t(Key::pid_header)).style(Style::default().fg(Color::Cyan)),
            Cell::from(t(Key::process_name_header)).style(Style::default().fg(Color::Cyan)),
            Cell::from(t(Key::cpu_header)).style(Style::default().fg(Color::Cyan)),
            Cell::from(t(Key::memory_header)).style(Style::default().fg(Color::Cyan)),
        ]))
        .block(
            Block::default()
                .title(td(DynKey::TableTitle {
                    name: app.process_name.clone(),
                    count: app.processes.len(),
                }))
                .borders(Borders::ALL),
        )
        .widths(&[Constraint::Length(8), Constraint::Min(10), Constraint::Length(8), Constraint::Min(0)]);

    f.render_widget(table, chunks[2]);

    // Help bar
    let help_text = match app.state {
        AppState::Normal => vec![
            Line::from(vec![
                Span::styled("↑/k", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_up)),
                Span::styled("↓/j", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_down)),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_kill_selected)),
                Span::styled("a", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_kill_high_mem)),
                Span::styled("r", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_refresh)),
                Span::styled("L", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_lang)),
                Span::styled("q", Style::default().fg(Color::Cyan)),
                Span::raw(t(Key::help_quit)),
            ]),
        ],
        AppState::ConfirmKill(index) => vec![
            Line::from(vec![
                Span::styled(
                    if index == usize::MAX {
                        t(Key::confirm_cleanup)
                    } else {
                        t(Key::confirm_kill_selected)
                    },
                    Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)
                ),
                Span::styled("y", Style::default().fg(Color::Green)),
                Span::raw(t(Key::confirm_yes)),
                Span::styled("n", Style::default().fg(Color::Red)),
                Span::raw(t(Key::confirm_no)),
            ]),
        ],
    };

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[3]);

    // Floating message
    if let Some(ref msg) = app.message {
        let msg_paragraph = Paragraph::new(msg.as_str())
            .block(Block::default())
            .alignment(Alignment::Center);
        let msg_area = Rect {
            x: size.x + size.width / 4,
            y: size.y + size.height / 2,
            width: size.width / 2,
            height: 3,
        };
        f.render_widget(Clear, msg_area);
        f.render_widget(msg_paragraph, msg_area);
    }
}
