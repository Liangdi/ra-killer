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

/// ra-killer: 自动监控并杀死高内存占用的 rust-analyzer 进程
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// 内存使用阈值 (百分比, 0-100)
    #[arg(short, long, default_value_t = 85, value_parser = clap::value_parser!(u8).range(1..100))]
    threshold: u8,

    /// 检查间隔 (秒)
    #[arg(short, long, default_value_t = 20, value_parser = clap::value_parser!(u64).range(5..3600))]
    interval: u64,

    /// 目标进程名
    #[arg(short, long, default_value = "rust-analyzer")]
    process: String,

    /// 只运行一次检查
    #[arg(long)]
    once: bool,

    /// 启用 TUI 模式
    #[arg(long)]
    tui: bool,

    /// 详细日志
    #[arg(long)]
    verbose: bool,
}

#[derive(Clone, Copy)]
enum AppState {
    Normal,
    ConfirmKill(usize),
}

/// 运行 TUI 界面
async fn run_tui(args: Args) -> Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(args.threshold, args.interval, args.process.clone());
    let mut last_tick = std::time::Instant::now();
    let tick_rate = Duration::from_millis(250);

    // 处理事件
    while !app.should_quit {
        // 清理过期消息
        app.clear_expired_message();

        // 绘制界面
        terminal.draw(|f| draw_ui(f, &app))?;

        // 处理输入
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
                                app.set_message("🔄 已刷新".to_string());
                            }
                            KeyCode::Char('a') => {
                                // 杀死所有进程
                                if !app.processes.is_empty() {
                                    app.state = AppState::ConfirmKill(usize::MAX);
                                }
                            }
                            _ => {}
                        }
                    }
                    AppState::ConfirmKill(index) => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Enter => {
                                if index == usize::MAX {
                                    // 杀死所有
                                    let count = app.processes.len();
                                    for proc in &app.processes {
                                        let _ = kill_process(proc.pid);
                                    }
                                    app.set_message(format!("✅ 已终止 {} 个进程", count));
                                    // 立即清空进程列表，给用户即时反馈
                                    app.processes.clear();
                                    app.selected_index = 0;
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

        // 自动刷新
        if last_tick.elapsed() >= Duration::from_secs(args.interval) {
            app.refresh_processes();
            last_tick = std::time::Instant::now();
        }
    }

    // 恢复终端
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

    // 初始化日志
    let log_level = if args.verbose {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    if !args.tui {
        tracing_subscriber::fmt().with_max_level(log_level).init();
    }

    if args.tui {
        // TUI 模式
        return run_tui(args).await;
    }

    // CLI 模式
    info!("🚀 ra-killer 启动");
    info!("📊 内存阈值: {}%", args.threshold);
    info!("⏱️  检查间隔: {} 秒", args.interval);
    info!("🎯 目标进程: {}", args.process);

    let mut sys = System::new_all();

    if args.once {
        // 只运行一次
        run_check(&mut sys, &args);
        return Ok(());
    }

    // 持续监控
    let mut timer = interval(Duration::from_secs(args.interval));

    loop {
        timer.tick().await;
        run_check(&mut sys, &args);
    }
}

fn run_check(sys: &mut System, args: &Args) {
    // 刷新系统信息
    sys.refresh_all();

    // 检查内存使用情况
    let total_memory = sys.total_memory();
    let used_memory = sys.used_memory();
    let memory_usage_percent = (used_memory as f64 / total_memory as f64 * 100.0) as u8;

    info!(
        "💾 内存使用: {} / {} ({}%)",
        format_bytes(used_memory),
        format_bytes(total_memory),
        memory_usage_percent
    );

    // 如果内存超过阈值，查找并杀死目标进程
    if memory_usage_percent >= args.threshold {
        warn!(
            "⚠️  内存使用率 {}% 超过阈值 {}%",
            memory_usage_percent, args.threshold
        );

        match find_and_kill_target_process(sys, &args.process) {
            Some(killed_count) => {
                if killed_count > 0 {
                    info!(
                        "✅ 已杀死 {} 个 {} 进程，释放内存",
                        killed_count, args.process
                    );
                } else {
                    info!("ℹ️  未找到 {} 进程", args.process);
                }
            }
            None => {
                error!("❌ 杀死进程时出错");
            }
        }
    } else {
        // 即使没超过阈值，也显示当前目标进程的内存使用情况
        show_target_processes(sys, &args.process);
    }
}

/// 查找并杀死目标进程
/// 返回 Some(成功杀死的进程数) 或 None(出错)
fn find_and_kill_target_process(sys: &System, target_name: &str) -> Option<usize> {
    let mut killed_count = 0;

    for (pid, process) in sys.processes() {
        if process
            .name()
            .to_string_lossy()
            .contains(target_name)
        {
            let memory_usage = process.memory();
            let pid = pid.as_u32();

            info!(
                "🔍 找到 {} 进程: PID={}, 内存={}",
                target_name,
                pid,
                format_bytes(memory_usage)
            );

            // 尝试优雅地终止进程
            if let Err(e) = kill_process(pid) {
                warn!("⚠️  无法终止进程 {}: {}", pid, e);
            } else {
                info!("✅ 已终止进程 {}", pid);
                killed_count += 1;
            }
        }
    }

    Some(killed_count)
}

/// 显示目标进程的内存使用情况
fn show_target_processes(sys: &System, target_name: &str) {
    let mut found = false;
    for (pid, process) in sys.processes() {
        if process
            .name()
            .to_string_lossy()
            .contains(target_name)
        {
            let memory_usage = process.memory();
            let pid = pid.as_u32();
            info!(
                "  📋 {} 进程: PID={}, 内存={}",
                target_name,
                pid,
                format_bytes(memory_usage)
            );
            found = true;
        }
    }

    if found {
        info!("  💡 提示: 使用 --once 或 -o 参数可以只运行一次检查");
    }
}

/// 终止指定进程
fn kill_process(pid: u32) -> Result<()> {
    use std::process::Command;

    // 使用 SIGTERM 优雅地终止进程
    let output = Command::new("kill")
        .arg("-15")
        .arg(pid.to_string())
        .output()?;

    if !output.status.success() {
        anyhow::bail!("kill 命令失败: {:?}", output.status);
    }

    Ok(())
}

/// 格式化字节数为可读格式
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

/// 进程信息结构
struct ProcessInfo {
    pid: u32,
    memory: u64,
    cpu: f32,
}

/// TUI 应用状态
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
        sys.refresh_all();
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
        // 重新创建 System 对象以获取最新的进程状态
        // refresh_all() 可能保留已死进程的缓存信息
        self.sys = System::new_all();

        // 清空进程列表
        self.processes.clear();

        // 重新获取进程列表
        for (pid, process) in self.sys.processes() {
            if process.name().to_string_lossy().contains(&self.process_name) {
                self.processes.push(ProcessInfo {
                    pid: pid.as_u32(),
                    memory: process.memory(),
                    cpu: process.cpu_usage(),
                });
            }
        }

        // 调整选中索引
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
        (self.used_memory() as f64 / self.total_memory() as f64) * 100.0
    }

    fn kill_selected_process(&mut self) -> Result<()> {
        if self.selected_index < self.processes.len() {
            let pid = self.processes[self.selected_index].pid;
            kill_process(pid)?;
            self.set_message(format!("✅ 已终止进程 {}", pid));
            self.refresh_processes();
        }
        Ok(())
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

/// 绘制 UI
fn draw_ui(f: &mut ratatui::Frame, app: &App) {
    let size = f.area();

    // 主布局
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3),  // 标题
                Constraint::Length(8),  // 系统信息
                Constraint::Min(0),     // 进程列表
                Constraint::Length(3),  // 帮助信息
            ]
            .as_ref(),
        )
        .split(size);

    // 标题
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
                "目标进程: ",
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

    // 系统信息
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
            Span::styled("💾 内存使用: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{} / {}", format_bytes(app.used_memory()), format_bytes(app.total_memory())),
                Style::default().fg(Color::White),
            ),
        ]),
        Line::from(vec![
            Span::styled("📊 使用率: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{:.1}%", memory_percent),
                Style::default().fg(gauge_color).add_modifier(Modifier::BOLD),
            ),
        ]),
        Line::from(vec![
            Span::styled("⚠️  阈值: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{}%", app.threshold),
                Style::default().fg(Color::Yellow),
            ),
        ]),
        Line::from(vec![
            Span::styled("🔄 刷新间隔: ", Style::default().fg(Color::Cyan)),
            Span::styled(
                format!("{} 秒", app.interval),
                Style::default().fg(Color::Gray),
            ),
        ]),
    ];

    let gauge = Gauge::default()
        .block(Block::default().title("系统状态").borders(Borders::ALL))
        .gauge_style(Style::default().fg(gauge_color).bg(Color::DarkGray))
        .percent(memory_percent as u16)
        .label(format!("{:.1}%", memory_percent));

    let sys_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
        .split(chunks[1]);

    let sys_info = Paragraph::new(system_info)
        .block(Block::default().title("系统信息").borders(Borders::ALL))
        .wrap(Wrap { trim: true });
    f.render_widget(sys_info, sys_chunks[0]);
    f.render_widget(gauge, sys_chunks[1]);

    // 进程列表
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
                Cell::from(format!("{:.1}%", p.cpu)),
                Cell::from(format_bytes(p.memory)),
            ])
            .style(style)
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(8), Constraint::Length(8), Constraint::Min(0)])
        .header(Row::new(vec![
            Cell::from("PID").style(Style::default().fg(Color::Cyan)),
            Cell::from("CPU").style(Style::default().fg(Color::Cyan)),
            Cell::from("内存").style(Style::default().fg(Color::Cyan)),
        ]))
        .block(
            Block::default()
                .title(format!(
                    "进程列表 - {} ({} 个)",
                    app.process_name,
                    app.processes.len()
                ))
                .borders(Borders::ALL),
        )
        .widths(&[Constraint::Length(8), Constraint::Length(8), Constraint::Min(0)]);

    f.render_widget(table, chunks[2]);

    // 帮助信息
    let help_text = match app.state {
        AppState::Normal => vec![
            Line::from(vec![
                Span::styled("↑/j", Style::default().fg(Color::Cyan)),
                Span::raw(" 上 "),
                Span::styled("↓/k", Style::default().fg(Color::Cyan)),
                Span::raw(" 下 "),
                Span::styled("Enter", Style::default().fg(Color::Cyan)),
                Span::raw(" 杀死选中 "),
                Span::styled("a", Style::default().fg(Color::Cyan)),
                Span::raw(" 杀死全部 "),
                Span::styled("r", Style::default().fg(Color::Cyan)),
                Span::raw(" 刷新 "),
                Span::styled("q", Style::default().fg(Color::Cyan)),
                Span::raw(" 退出"),
            ]),
        ],
        AppState::ConfirmKill(_) => vec![
            Line::from(vec![
                Span::styled("确认杀死进程? ", Style::default().fg(Color::Yellow).add_modifier(Modifier::BOLD)),
                Span::styled("y", Style::default().fg(Color::Green)),
                Span::raw(" 是 "),
                Span::styled("n", Style::default().fg(Color::Red)),
                Span::raw(" 否"),
            ]),
        ],
    };

    let help = Paragraph::new(help_text)
        .block(Block::default().borders(Borders::ALL))
        .alignment(Alignment::Center);
    f.render_widget(help, chunks[3]);

    // 显示消息
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
