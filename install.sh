#!/bin/bash
# 安装 ra-killer 到系统

set -e

echo "🔧 正在安装 ra-killer..."

# 编译 release 版本
cargo build --release

# 复制到 /usr/local/bin (需要 sudo)
sudo cp target/release/ra-killer /usr/local/bin/

# 安装 systemd service (可选)
if [ -f ra-killer.service ]; then
    echo "📋 安装 systemd service..."
    sudo cp ra-killer.service /etc/systemd/system/
    sudo systemctl daemon-reload
    sudo systemctl enable ra-killer
    echo "✅ 安装完成！使用以下命令管理服务："
    echo "   启动: sudo systemctl start ra-killer"
    echo "   停止: sudo systemctl stop ra-killer"
    echo "   状态: sudo systemctl status ra-killer"
    echo "   日志: sudo journalctl -u ra-killer -f"
else
    echo "✅ 安装完成！现在你可以使用 'ra-killer' 命令了"
fi
