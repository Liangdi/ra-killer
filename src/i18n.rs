use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Zh = 1,
    En = 2,
}

/// 0 = 未初始化, 1 = Zh, 2 = En
static LANG_CODE: AtomicU8 = AtomicU8::new(0);

/// 从字符串推断语言
fn lang_from_str(s: &str) -> Lang {
    match s.to_lowercase().as_str() {
        "en" | "en-us" | "en-gb" | "english" => Lang::En,
        _ => Lang::Zh,
    }
}

/// 自动检测系统语言
fn auto_detect() -> Lang {
    if let Ok(val) = std::env::var("RA_KILLER_LANG") {
        return lang_from_str(&val);
    }
    if let Some(locale) = sys_locale::get_locale() {
        if locale.starts_with("zh") {
            return Lang::Zh;
        }
        return Lang::En;
    }
    Lang::Zh
}

/// 初始化语言设置（可在 main 早期调用）
pub fn init(lang_hint: Option<&str>) {
    let lang = match lang_hint {
        Some(s) if !s.is_empty() && s != "auto" => lang_from_str(s),
        _ => auto_detect(),
    };
    LANG_CODE.store(lang as u8, Ordering::SeqCst);
}

/// 获取当前语言
pub fn lang() -> Lang {
    let code = LANG_CODE.load(Ordering::SeqCst);
    if code == 0 {
        let detected = auto_detect();
        LANG_CODE.store(detected as u8, Ordering::SeqCst);
        detected
    } else {
        match code {
            2 => Lang::En,
            _ => Lang::Zh,
        }
    }
}

/// 切换语言（TUI 快捷键用）
pub fn toggle() {
    let current = lang();
    let new_lang = match current {
        Lang::Zh => Lang::En,
        Lang::En => Lang::Zh,
    };
    LANG_CODE.store(new_lang as u8, Ordering::SeqCst);
}

// ============================================================
// 静态字符串 Key
// ============================================================

#[derive(Clone, Copy)]
#[allow(non_camel_case_types)]
pub enum Key {
    // TUI 标题区
    target_process_label,
    // TUI 系统信息标签
    memory_usage_label,
    usage_label,
    threshold_label,
    refresh_interval_label,
    auto_cleanup_label,
    // TUI 系统信息值
    auto_cleanup_enabled,
    // TUI 面板标题
    system_status,
    system_info,
    // TUI 表头
    pid_header,
    process_name_header,
    cpu_header,
    memory_header,
    // TUI 帮助栏
    help_up,
    help_down,
    help_kill_selected,
    help_kill_high_mem,
    help_refresh,
    help_quit,
    help_lang,
    // TUI 确认对话框
    confirm_cleanup,
    confirm_kill_selected,
    confirm_yes,
    confirm_no,
    // TUI 消息
    msg_refreshed,
    msg_lang_switched,
    // CLI 启动
    cli_starting,
    // CLI 日志
    cli_kill_error,
    cli_hint_once,
    // 错误消息
    err_kill_failed,
    err_sigkill_failed,
}

/// TUI 静态文案。sci-fi HUD 风格不使用 emoji，改用简短遥测式标签；
/// CLI 相关文案（cli_* / err_*）保留原 emoji 不变。
pub fn t(key: Key) -> &'static str {
    use Key::*;
    match lang() {
        Lang::Zh => match key {
            target_process_label => "目标进程",
            memory_usage_label => "内存使用",
            usage_label => "使用率",
            threshold_label => "阈值",
            refresh_interval_label => "刷新间隔",
            auto_cleanup_label => "自动清理",
            auto_cleanup_enabled => "开启 · 清理内存最高 40%",
            system_status => "系统状态",
            system_info => "系统信息",
            pid_header => "PID",
            process_name_header => "进程名",
            cpu_header => "CPU",
            memory_header => "内存",
            help_up => " 上 ",
            help_down => " 下 ",
            help_kill_selected => " 杀死选中 ",
            help_kill_high_mem => " 清理高内存(40%) ",
            help_refresh => " 刷新 ",
            help_quit => " 退出",
            help_lang => " 切换语言",
            confirm_cleanup => "确认清理内存最高的 40% 进程? ",
            confirm_kill_selected => "确认杀死选中进程? ",
            confirm_yes => " 是 ",
            confirm_no => " 否",
            msg_refreshed => "» 已刷新",
            msg_lang_switched => "● 语言已切换为中文",
            cli_starting => "🚀 ra-killer 启动",
            cli_kill_error => "❌ 杀死进程时出错",
            cli_hint_once => "  💡 提示: 使用 --once 或 -o 参数可以只运行一次检查",
            err_kill_failed => "kill 命令失败",
            err_sigkill_failed => "SIGKILL 失败",
        },
        Lang::En => match key {
            target_process_label => "TARGET",
            memory_usage_label => "MEMORY",
            usage_label => "USAGE",
            threshold_label => "THRESHOLD",
            refresh_interval_label => "REFRESH",
            auto_cleanup_label => "AUTO CLEANUP",
            auto_cleanup_enabled => "ON · kill top 40% memory",
            system_status => "SYSTEM STATUS",
            system_info => "SYSTEM INFO",
            pid_header => "PID",
            process_name_header => "PROCESS",
            cpu_header => "CPU",
            memory_header => "MEMORY",
            help_up => " Up ",
            help_down => " Down ",
            help_kill_selected => " Kill Sel ",
            help_kill_high_mem => " Kill Top 40% ",
            help_refresh => " Refresh ",
            help_quit => " Quit",
            help_lang => " Language",
            confirm_cleanup => "Confirm kill top 40% memory processes? ",
            confirm_kill_selected => "Confirm kill selected process? ",
            confirm_yes => " Yes ",
            confirm_no => " No",
            msg_refreshed => "» Refreshed",
            msg_lang_switched => "● Language switched to English",
            cli_starting => "🚀 ra-killer started",
            cli_kill_error => "❌ Error killing processes",
            cli_hint_once => "  💡 Tip: Use --once or -o to run a single check",
            err_kill_failed => "kill command failed",
            err_sigkill_failed => "SIGKILL failed",
        },
    }
}

// ============================================================
// 动态字符串 Key（带运行时参数）
// ============================================================

#[derive(Clone)]
pub enum DynKey {
    // TUI 消息
    MsgKilledCount(usize),
    MsgKillFailed(String),
    MsgAutoKilled(usize),
    MsgKilledProcess(u32),
    // TUI 格式
    TableTitle { name: String, count: usize },
    RefreshIntervalSecs(u64),
    // CLI 启动
    CliThreshold(u8),
    CliInterval(u64),
    CliTarget(String),
    // CLI 日志
    CliMemoryUsage { used: String, total: String, percent: f64 },
    CliMemoryWarning { percent: f64, threshold: u8 },
    CliKilledReleased { count: usize, name: String },
    CliNoProcess(String),
    CliFoundDetail { name: String, pid: u32, mem: String },
    CliCannotTerminate { pid: u32, error: String },
    CliTerminated(u32),
    CliProcessDetail { name: String, pid: u32, mem: String },
    CliSigtermFailed(u32),
}

/// TUI 动态消息使用 sci-fi 状态符号（» ● [OK] ✕）；CLI 日志（Cli*）保留 emoji。
pub fn td(key: DynKey) -> String {
    use DynKey::*;
    match lang() {
        Lang::Zh => match key {
            MsgKilledCount(n) => format!("[OK] 已终止 {} 个进程（内存最高 40%）", n),
            MsgKillFailed(e) => format!("✕ 终止进程失败: {}", e),
            MsgAutoKilled(n) => format!("» 已自动终止 {} 个高内存进程", n),
            MsgKilledProcess(pid) => format!("[OK] 已终止进程 {}", pid),
            TableTitle { name, count } => format!("进程列表 - {} ({} 个)", name, count),
            RefreshIntervalSecs(s) => format!("{} 秒", s),
            CliThreshold(v) => format!("📊 内存阈值: {}%", v),
            CliInterval(v) => format!("⏱️  检查间隔: {} 秒", v),
            CliTarget(v) => format!("🎯 目标进程: {}", v),
            CliMemoryUsage { used, total, percent } => {
                format!("💾 内存使用: {} / {} ({:.1}%)", used, total, percent)
            }
            CliMemoryWarning { percent, threshold } => {
                format!("⚠️  内存使用率 {:.1}% 超过阈值 {}%", percent, threshold)
            }
            CliKilledReleased { count, name } => {
                format!("✅ 已杀死 {} 个 {} 进程，释放内存", count, name)
            }
            CliNoProcess(name) => format!("ℹ️  未找到 {} 进程", name),
            CliFoundDetail { name, pid, mem } => {
                format!("🔍 找到 {} 进程: PID={}, 内存={}", name, pid, mem)
            }
            CliCannotTerminate { pid, error } => {
                format!("⚠️  无法终止进程 {}: {}", pid, error)
            }
            CliTerminated(pid) => format!("✅ 已终止进程 {}", pid),
            CliProcessDetail { name, pid, mem } => {
                format!("  📋 {} 进程: PID={}, 内存={}", name, pid, mem)
            }
            CliSigtermFailed(pid) => {
                format!("⚠️  进程 {} 未响应 SIGTERM，使用 SIGKILL", pid)
            }
        },
        Lang::En => match key {
            MsgKilledCount(n) => format!("[OK] Killed {} processes (top 40% memory)", n),
            MsgKillFailed(e) => format!("✕ Failed to kill processes: {}", e),
            MsgAutoKilled(n) => format!("» Auto-killed {} high-memory processes", n),
            MsgKilledProcess(pid) => format!("[OK] Terminated process {}", pid),
            TableTitle { name, count } => format!("Processes - {} ({} total)", name, count),
            RefreshIntervalSecs(s) => format!("{}s", s),
            CliThreshold(v) => format!("📊 Memory threshold: {}%", v),
            CliInterval(v) => format!("⏱️  Check interval: {}s", v),
            CliTarget(v) => format!("🎯 Target process: {}", v),
            CliMemoryUsage { used, total, percent } => {
                format!("💾 Memory: {} / {} ({:.1}%)", used, total, percent)
            }
            CliMemoryWarning { percent, threshold } => {
                format!("⚠️  Memory usage {:.1}% exceeds threshold {}%", percent, threshold)
            }
            CliKilledReleased { count, name } => {
                format!("✅ Killed {} {} processes, memory freed", count, name)
            }
            CliNoProcess(name) => format!("ℹ️  No {} processes found", name),
            CliFoundDetail { name, pid, mem } => {
                format!("🔍 Found {} process: PID={}, Memory={}", name, pid, mem)
            }
            CliCannotTerminate { pid, error } => {
                format!("⚠️  Cannot terminate process {}: {}", pid, error)
            }
            CliTerminated(pid) => format!("✅ Terminated process {}", pid),
            CliProcessDetail { name, pid, mem } => {
                format!("  📋 {} process: PID={}, Memory={}", name, pid, mem)
            }
            CliSigtermFailed(pid) => {
                format!("⚠️  Process {} did not respond to SIGTERM, using SIGKILL", pid)
            }
        },
    }
}
