/**
 * Generate icons from SVG source.
 * Run: npx tsx scripts/generate-icons.ts
 */

import sharp from 'sharp';
import { promises as fs } from 'fs';

const SIZES = [32, 64, 128, 256, 512, 1024];
const ICONS_DIR = 'src-tauri/icons';

async function generateIcons() {
  const svgBuffer = await fs.readFile(`${ICONS_DIR}/icon.svg`);

  // Generate PNGs for different sizes
  for (const size of SIZES) {
    await sharp(svgBuffer)
      .resize(size, size)
      .png()
      .toFile(`${ICONS_DIR}/${size}x${size}.png`);
    console.log(`Generated ${size}x${size}.png`);
  }

  // Generate special sizes
  await sharp(svgBuffer)
    .resize(128, 128)
    .png()
    .toFile(`${ICONS_DIR}/128x128@2x.png`);
  console.log('Generated 128x128@2x.png');

  await sharp(svgBuffer)
    .resize(512, 512)
    .png()
    .toFile(`${ICONS_DIR}/icon.png`);
  console.log('Generated icon.png (512x512)');

  // Generate macOS .icns (using iconutil requires macOS)
  // For cross-platform, you'd need a dedicated tool like png2icns or electron-icon-maker
  console.log('\nNote: For .icns and .ico, use dedicated tools:');
  console.log('  macOS: iconutil -c icns src-tauri/icons/icon.iconset');
  console.log('  Cross-platform: npm install -g @electron/packager && electron-icon-maker');

  console.log('\n✅ Basic PNG icons generated successfully!');
}

generateIcons().catch(console.error);
