#!/bin/bash

echo "🎨 Token Usage 图标快速设置"
echo ""

# 创建图标目录
mkdir -p src-tauri/icons 2>/dev/null

echo "📋 现在请："
echo ""
echo "   1. 浏览器中已打开「图标生成器」"
echo "   2. 点击「下载 App 图标」按钮"
echo "   3. 下载完成后，运行以下命令："
echo ""
echo "   # 复制 App 图标（假设在下载文件夹）"
    echo "   cp ~/Downloads/icon.png src-tauri/icons/"
echo ""
echo "   # 测试应用"
    echo "   npm run tauri dev"
echo ""

echo "💡 图标规格："
echo "   • App 图标：蓝色渐变背景 + 白色加粗 T"
echo "   • 蓝色：#0066FF → #0052CC（微妙渐变）"
echo "   • 字体：SF Pro Display，font-weight: 700（加粗）"
echo ""
