import { defineConfig } from "vite";
import { svelte } from "@sveltejs/vite-plugin-svelte";
import pkg from "./package.json" with { type: "json" };

export default defineConfig(async () => {
  const HOST = process.env.TAURI_DEV_HOST;
  const env = {
    VITE_APP_VERSION: pkg.version,
    VITE_UPDATE_REPO: process.env.VITE_UPDATE_REPO || "gitee.com/lzc0088/token-usage",
  };

  return {
    plugins: [svelte()],
    test: {
      include: ["src/**/*.{test,spec}.{ts,js}"],
      environment: "jsdom",
    },
    // Dependency scan: only the real app entry. Without this the scanner
    // crawls every HTML it can find (docs/, assets/ design scripts, even
    // src-tauri/target build artifacts), which is slow and breaks on files
    // that aren't part of the app bundle.
    optimizeDeps: {
      entries: ["index.html"],
    },
    envPrefix: ["VITE_"],
    env,

    // Vite options for Tauri: no clearing of screen, fixed port, relative base.
    clearScreen: false,
    server: {
      port: 1420,
      strictPort: true,
      host: HOST || false,
      hmr: HOST
        ? { protocol: "ws", host: HOST, port: 1421 }
        : undefined,
      watch: {
        // Don't watch the Rust source; Tauri handles its own reload.
        ignored: ["**/src-tauri/**"],
      },
    },
    // Produce sourcemaps for debug builds.
    build: {
      target: "es2021",
      sourcemap: !!process.env.TAURI_DEBUG,
    },
  };
});
