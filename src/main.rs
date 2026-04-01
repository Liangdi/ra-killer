use anyhow::Result;
use clap::Parser;
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

    /// 详细日志
    #[arg(long)]
    verbose: bool,
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

    tracing_subscriber::fmt().with_max_level(log_level).init();

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
