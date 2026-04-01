# ra-killer

> 自动监控并杀死高内存占用的 rust-analyzer 进程

## 📖 简介

`ra-killer` 是一个专门为 Rust 开发者设计的内存监控工具。当系统内存使用率超过设定阈值时，它会自动查找并终止占用大量内存的 `rust-analyzer` 进程，防止系统因内存不足而卡顿或崩溃。

### 为什么需要这个工具？

`rust-analyzer` 是 Rust 优秀的语言服务器，但在开发大型项目时，它可能会占用大量内存（有时每个进程可达 2-3 GB）。当同时打开多个 Rust 项目时，这些进程会迅速消耗系统资源，导致：
- 系统变慢或卡顿
- 编辑器响应延迟
- 其他程序无法正常运行
- 系统触发 OOM (Out of Memory)

`ra-killer` 通过智能监控和自动清理，解决这些问题。

## ✨ 特性

- 🔍 **实时监控** - 定期检查系统内存使用情况
- 🎯 **智能清理** - 自动终止高内存占用的 rust-analyzer 进程
- ⚙️ **灵活配置** - 支持自定义内存阈值、检查间隔等参数
- 📊 **详细日志** - 显示内存使用和进程信息
- 🚀 **轻量高效** - 优化的二进制文件，仅 883 KB
- 🔄 **持久运行** - 支持 systemd service 后台运行

## 📦 安装

### 从源码编译

```bash
# 克隆仓库
git clone <repository-url>
cd ra-killer

# 编译 release 版本
cargo build --release

# 二进制文件位于 target/release/ra-killer
```

### 安装到系统

```bash
# 使用提供的安装脚本
sudo ./install.sh

# 或手动复制
sudo cp target/release/ra-killer /usr/local/bin/
```

### 作为 systemd 服务运行

```bash
# 复制 service 文件
sudo cp ra-killer.service /etc/systemd/system/

# 重载 systemd 配置
sudo systemctl daemon-reload

# 启用并启动服务
sudo systemctl enable ra-killer
sudo systemctl start ra-killer

# 查看服务状态
sudo systemctl status ra-killer

# 查看日志
sudo journalctl -u ra-killer -f
```

## 🚀 使用方法

### 基本使用

```bash
# 使用默认配置运行（内存阈值 85%，检查间隔 20 秒）
ra-killer

# 只运行一次检查
ra-killer --once

# 显示帮助信息
ra-killer --help

# 显示版本信息
ra-killer --version
```

### 常用选项

```bash
# 自定义内存阈值为 90%
ra-killer --threshold 90

# 设置检查间隔为 60 秒
ra-killer --interval 60

# 组合使用：每 10 秒检查一次，阈值为 80%
ra-killer -i 10 -t 80

# 监控其他进程（如 node）
ra-killer --process node

# 显示详细日志
ra-killer --verbose
```

### 命令行参数

| 参数 | 简写 | 默认值 | 说明 |
|------|------|--------|------|
| `--threshold <百分比>` | `-t` | 85 | 内存使用阈值（0-100），超过时触发清理 |
| `--interval <秒数>` | `-i` | 20 | 检查间隔时间（秒），最小 5 秒 |
| `--process <进程名>` | `-p` | rust-analyzer | 要监控的目标进程名 |
| `--once` | `-o` | - | 只运行一次检查后退出 |
| `--verbose` | `-v` | - | 显示详细的调试日志 |
| `--help` | `-h` | - | 显示帮助信息 |
| `--version` | `-V` | - | 显示版本信息 |

## 📊 输出示例

```
2026-04-01T09:36:57.791668Z  INFO ra_killer: 🚀 ra-killer 启动
2026-04-01T09:36:57.791683Z  INFO ra_killer: 📊 内存阈值: 85%
2026-04-01T09:36:57.791685Z  INFO ra_killer: ⏱️  检查间隔: 20 秒
2026-04-01T09:36:57.791686Z  INFO ra_killer: 🎯 目标进程: rust-analyzer
2026-04-01T09:36:57.887637Z  INFO ra_killer: 💾 内存使用: 20.77 GB / 31.06 GB (66%)
2026-04-01T09:36:57.887673Z  INFO ra_killer:   📋 rust-analyzer 进程: PID=32918, 内存=1.07 GB
2026-04-01T09:36:57.887697Z  INFO ra_killer:   📋 rust-analyzer 进程: PID=8157, 内存=2.42 GB
```

当内存超过阈值时：
```
2026-04-01T09:40:00.123456Z  WARN ra_killer: ⚠️  内存使用率 88% 超过阈值 85%
2026-04-01T09:40:00.123789Z  INFO ra_killer: 🔍 找到 rust-analyzer 进程: PID=8157, 内存=2.42 GB
2026-04-01T09:40:00.234567Z  INFO ra_killer: ✅ 已终止进程 8157
2026-04-01T09:40:00.234789Z  INFO ra_killer: ✅ 已杀死 2 个 rust-analyzer 进程，释放内存
```

## ⚙️ 配置建议

### 开发环境

对于开发机器，建议使用较宽松的配置：

```bash
ra-killer --threshold 85 --interval 60
```

- 阈值 85%：留有足够余量，避免误杀
- 间隔 60 秒：降低检查频率，减少系统开销

### 低内存系统

对于内存较小的系统（< 8GB），使用更严格的配置：

```bash
ra-killer --threshold 75 --interval 30
```

- 阈值 75%：更早触发清理，防止 OOM
- 间隔 20 秒：合理的检查频率，及时响应

### 服务器环境

对于持续运行的服务器，建议使用 systemd service 并调整配置：

```bash
# 编辑 ra-killer.service
ExecStart=/usr/local/bin/ra-killer --threshold 80 --interval 120
```

## 🛠️ 开发

### 构建

```bash
# 开发构建
cargo build

# Release 构建（包含尺寸优化）
cargo build --release
```

### 测试

```bash
# 运行单次检查
cargo run -- --once

# 运行完整监控
cargo run -- --interval 10 --threshold 70
```

### 优化

项目已配置了 Release 模式的尺寸优化：

- `opt-level = "z"` - 优化二进制大小
- `lto = true` - 链接时优化
- `strip = true` - 去除调试符号
- `panic = "abort"` - 减小 panic 处理开销

最终二进制文件大小约 **883 KB**。

## 🔍 工作原理

1. **定期检查**：根据设定的间隔时间，定期获取系统内存使用情况
2. **阈值判断**：比较当前内存使用率与设定阈值
3. **进程查找**：当超过阈值时，查找所有目标进程（rust-analyzer）
4. **显示信息**：输出每个进程的 PID 和内存占用
5. **终止进程**：使用 `SIGTERM` 信号优雅地终止进程
6. **内存释放**：进程终止后，系统自动回收内存

## ⚠️ 注意事项

1. **数据安全**：进程终止是立即执行的，请确保已保存工作
2. **阈值设置**：建议根据实际情况调整阈值，避免过于频繁的清理
3. **进程恢复**：rust-analyzer 会在需要时自动重启，无需手动干预
4. **权限要求**：终止进程可能需要适当的权限
5. **日志监控**：建议定期查看日志，了解工具运行情况

## 🤝 贡献

欢迎提交 Issue 和 Pull Request！

### 开发建议

- 保持代码简洁高效
- 添加适当的错误处理
- 更新文档和注释
- 遵循 Rust 代码规范

## 📄 许可证

[MIT License](LICENSE)

## 🙏 致谢

- [sysinfo](https://github.com/GuillaumeGomez/sysinfo) - 系统信息获取
- [tokio](https://tokio.rs/) - 异步运行时
- [clap](https://github.com/clap-rs/clap) - 命令行参数解析
