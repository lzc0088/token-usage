#!/bin/bash

echo "🎨 Token Usage 图标设置向导"
echo ""

# 检查是否在项目根目录
if [ ! -d "src-tauri" ]; then
    echo "❌ 错误：请在项目根目录运行此脚本"
    echo "   当前目录: $(pwd)"
    echo "   应该在: /Users/zechuan/work/workspace/projects/ai-agent/token-usage"
    exit 1
fi

echo "📋 步骤 1: 生成图标"
echo ""
echo "   在浏览器中打开: file://$(pwd)/icon-generator-final.html"
echo ""
open "file://$(pwd)/icon-generator-final.html"

echo ""
echo "📋 步骤 2: 下载图标"
echo ""
echo "   在浏览器中："
echo "   - 点击「下载菜单栏图标」获取 16x16.png"
echo "   - 点击「下载 App 图标」获取 icon.png"
echo ""

echo "📋 步骤 3: 复制图标"
echo ""
echo "   运行以下命令："
echo ""
echo "   # 创建图标目录"
echo "   mkdir -p src-tauri/icons"
echo ""
echo "   # 复制菜单栏图标（假设下载在 ~/Downloads）"
    echo "   cp ~/Downloads/16x16.png src-tauri/icons/32x32.png"
echo ""
    echo "   # 复制 App 图标"
    echo "   cp ~/Downloads/icon.png src-tauri/icons/icon.png"
echo ""

echo "📋 步骤 4: 清理缓存并测试"
echo ""
echo "   # 清理构建缓存"
    echo "   rm -rf src-tauri/target"
echo ""
echo "   # 测试应用"
    echo "   npm run tauri dev"
echo ""

echo "✅ 图标设置完成后，检查："
echo "   • 菜单栏图标是否显示（全白T）"
echo "   • App 图标是否显示（蓝底白T）"
echo ""
echo "💡 提示："
echo "   - 菜单栏图标：全白色，适配深色菜单栏"
echo "   - App 图标：蓝色渐变 (#0066FF → #0052CC)，白色加粗 T"
