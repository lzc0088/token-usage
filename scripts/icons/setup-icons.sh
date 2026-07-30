#!/bin/bash

# 图标设置脚本 - Token Usage

echo "📋 为 Token Usage 设置图标..."

# 创建图标目录
mkdir -p src-tauri/icons

# 假设用户从 icon-generator-final.html 下载了文件到 ~/Downloads
DOWNLOAD_DIR="$HOME/Downloads"

# 检查下载的文件是否存在
if [ ! -f "$DOWNLOAD_DIR/512x512.png" ]; then
    echo "❌ 错误：找不到下载的图标文件"
    echo "请先打开 icon-generator-final.html 下载 App 图标 (512×512)"
    echo "   或者手动提供图标文件路径"
    exit 1
fi

# 复制主应用图标
echo "📱 复制 App 图标..."
cp "$DOWNLOAD_DIR/512x512.png" src-tauri/icons/icon.png
echo "✅ icon.png (512×512, 蓝底白T)"

# 如果有其他尺寸，也复制
if [ -f "$DOWNLOAD_DIR/256x256.png" ]; then
    cp "$DOWNLOAD_DIR/256x256.png" src-tauri/icons/256x256.png
    echo "✅ 256x256.png"
fi

if [ -f "$DOWNLOAD_DIR/128x128.png" ]; then
    cp "$DOWNLOAD_DIR/128x128.png" src-tauri/icons/128x128.png
    echo "✅ 128x128.png"
fi

if [ -f "$DOWNLOAD_DIR/64x64.png" ]; then
    cp "$DOWNLOAD_DIR/64x64.png" src-tauri/icons/64x64.png
    echo "✅ 64x64.png"
fi

if [ -f "$DOWNLOAD_DIR/16x16.png" ]; then
    cp "$DOWNLOAD_DIR/16x16.png" src-tauri/icons/32x32.png
    echo "✅ 32x32.png (菜单栏图标)"
fi

echo ""
echo "🎯 图标设置完成！"
echo ""
echo "📝 接下来："
echo "   1. npm run tauri dev  # 测试应用"
echo "   2. 检查菜单栏图标和 App 图标是否正确显示"
echo ""
echo "📦 图标文件位置："
echo "   - src-tauri/icons/icon.png (主图标，蓝底白T)"
echo "   - src-tauri/icons/32x32.png (菜单栏图标，全白T)"
