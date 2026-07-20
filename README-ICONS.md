# Icon Generation Guide

## Current Status
✅ SVG icon created at `src-tauri/icons/icon.svg`
⏳ Need to convert to PNG/ICNS/ICO formats

## Design
The icon features:
- **Letter "T"** in amber (#e8b04b) representing "Token"
- **Bar chart elements** at bottom in multiple colors (lime, amber, cyan, violet)
- **Dark gradient background** for menu bar visibility
- **Rounded corners** for modern aesthetic
- **512x512 base resolution**

## Conversion Options

### Option 1: Online Tools (Easiest)
1. Visit [CloudConvert](https://cloudconvert.com/svg-to-png) or [Convertio](https://convertio.io/svg-png)
2. Upload `src-tauri/icons/icon.svg`
3. Convert to PNG at various sizes: 32x32, 128x128, 512x512
4. Use [ICO Convert](https://icoconvert.com/) for .ico (Windows)
5. Use [Img2ICNS](https://www.img2icnsapp.com/) for .icns (macOS)

### Option 2: Command Line (macOS)
```bash
# Install librsvg
brew install librsvg

# Convert SVG to PNG
rsvg-convert -w 32 -h 32 src-tauri/icons/icon.svg -o src-tauri/icons/32x32.png
rsvg-convert -w 128 -h 128 src-tauri/icons/icon.svg -o src-tauri/icons/128x128.png
rsvg-convert -w 512 -h 512 src-tauri/icons/icon.svg -o src-tauri/icons/icon.png

# Create iconset for macOS
mkdir -p src-tauri/icons/icon.iconset
rsvg-convert -w 16 -h 16 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_16x16.png
rsvg-convert -w 32 -h 32 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_16x16@2x.png
rsvg-convert -w 32 -h 32 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_32x32.png
rsvg-convert -w 64 -h 64 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_32x32@2x.png
rsvg-convert -w 128 -h 128 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_128x128.png
rsvg-convert -w 256 -h 256 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_128x128@2x.png
rsvg-convert -w 256 -h 256 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_256x256.png
rsvg-convert -w 512 -h 512 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_256x256@2x.png
rsvg-convert -w 512 -h 512 src-tauri/icons/icon.svg -o src-tauri/icons/icon.iconset/icon_512x512.png

# Build .icns
iconutil -c icns src-tauri/icons/icon.iconset -o src-tauri/icons/icon.icns
```

### Option 3: Node.js Script
```bash
npm install --save-dev sharp tsx
npx tsx scripts/generate-icons.ts
```

### Option 4: Professional Tools
- **Sketch/Figma**: Export SVG to multiple formats
- **Photoshop**: Place SVG and export as PNG/ICO
- **GIMP**: Open SVG and export to various formats

## Required Files
Replace the default Tauri icons:
- `32x32.png` - Menu bar icon (smallest)
- `128x128.png` - App icon
- `128x128@2x.png` - Retina display
- `icon.png` - Main app icon (512x512)
- `icon.icns` - macOS bundle icon
- `icon.ico` - Windows executable icon

## Testing
After replacing icons:
```bash
npm run tauri dev
```
Check the menu bar icon and app dock icon to verify visibility and aesthetics.

## Icon Design Guidelines
- **Contrast**: Ensure good contrast in both light and dark menu bars
- **Simplicity**: Icon should be recognizable at 16x16 pixels
- **Thematic**: Bar chart elements represent token usage statistics
- **Colors**: Use project theme colors (amber #e8b04b, lime #b4e34c)

## Maintenance
If you need to modify the icon:
1. Edit `src-tauri/icons/icon.svg`
2. Re-run conversion process
3. Test in both light and dark modes
