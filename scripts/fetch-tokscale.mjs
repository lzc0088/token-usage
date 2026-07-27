// Fetches the @tokscale/cli-{triple} native binary for the current platform
// (or an explicit --target) and places it at src-tauri/bin/tokscale[.exe],
// ready for Tauri's bundle.resources to pick up.
//
// Env:
//   TOKSCALE_REGISTRY  override npm registry (default: https://registry.npmmirror.com)
//   TOKSCALE_VERSION   override version (default: read from tokscale.rs::TOKSCALE_VERSION)
//   TAURI_TARGET       rust target triple, e.g. aarch64-apple-darwin (overrides host)
// Argv:
//   --target=<triple>  same as TAURI_TARGET
//   --registry=<url>   same as TOKSCALE_REGISTRY
//   --version=<x.y.z>  same as TOKSCALE_VERSION
//
// Idempotent: skips if the output already exists (delete to re-fetch).

import { createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, renameSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import https from "node:https";

const __dirname = dirname(fileURLToPath(import.meta.url));
const ROOT = join(__dirname, "..");
const OUT_DIR = join(ROOT, "src-tauri", "bin");
const RS_PATH = join(ROOT, "src-tauri", "src", "collector", "tokscale.rs");

const DEFAULT_REGISTRY = "https://registry.npmmirror.com"; // CN-friendly, globally reachable

// ── argv + env ───────────────────────────────────────────────────────────────
function parseOpts() {
  const opts = {
    version: process.env.TOKSCALE_VERSION || null,
    registry: process.env.TOKSCALE_REGISTRY || DEFAULT_REGISTRY,
    target: process.env.TAURI_TARGET || null,
  };
  for (const a of process.argv.slice(2)) {
    const m = /^--([^=]+)=(.*)$/.exec(a);
    if (!m) continue;
    const [, k, v] = m;
    if (k === "target") opts.target = v;
    else if (k === "registry") opts.registry = v;
    else if (k === "version") opts.version = v;
  }
  return opts;
}

// tokscale.rs is the single source of truth for the pinned version.
// Read it unless the caller explicitly overrode the version.
function pinnedVersion() {
  const src = readFileSync(RS_PATH, "utf8");
  const m = /pub const TOKSCALE_VERSION:\s*&str\s*=\s*"([^"]+)"/.exec(src);
  if (!m) throw new Error(`cannot find TOKSCALE_VERSION in ${RS_PATH}`);
  return m[1];
}

// rust target triple → npm @tokscale/cli-<suffix>. Mirrors tokscale.rs::platform_triple.
function npmTriple(rustTarget) {
  const map = {
    "aarch64-apple-darwin": "darwin-arm64",
    "x86_64-apple-darwin": "darwin-x64",
    "x86_64-unknown-linux-gnu": "linux-x64-gnu",
    "x86_64-unknown-linux-musl": "linux-x64-musl",
    "aarch64-unknown-linux-gnu": "linux-arm64-gnu",
    "aarch64-unknown-linux-musl": "linux-arm64-musl",
    "x86_64-pc-windows-msvc": "win32-x64-msvc",
    "aarch64-pc-windows-msvc": "win32-arm64-msvc",
  };
  if (rustTarget) {
    const t = map[rustTarget];
    if (!t) throw new Error(`unknown rust target: ${rustTarget}`);
    return t;
  }
  // fall back to current host (dev on matching platform)
  const { arch, platform } = process;
  if (platform === "darwin") return arch === "arm64" ? "darwin-arm64" : "darwin-x64";
  if (platform === "linux") return arch === "arm64" ? "linux-arm64-gnu" : "linux-x64-gnu";
  if (platform === "win32") return arch === "arm64" ? "win32-arm64-msvc" : "win32-x64-msvc";
  throw new Error(`unsupported host: ${platform}/${arch}`);
}

// HTTPS GET following up to 3 redirects (npmmirror redirects tarballs to a CDN).
function get(url, redirects = 0) {
  return new Promise((resolve, reject) => {
    if (redirects > 3) return reject(new Error(`too many redirects for ${url}`));
    https.get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        return resolve(get(res.headers.location, redirects + 1));
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      resolve(res);
    }).on("error", reject);
  });
}

async function download(url, dest) {
  const stream = await get(url);
  await new Promise((resolve, reject) => {
    stream.pipe(createWriteStream(dest)).on("finish", resolve).on("error", reject);
  });
}

// Use the system tar (mac/linux + Win10+) to extract the tarball.
function extractBinary(tgzPath, binName) {
  const tmp = join(tmpdir(), `tokscale-extract-${process.pid}`);
  rmSync(tmp, { recursive: true, force: true });
  mkdirSync(tmp, { recursive: true });
  const r = spawnSync("tar", ["-xzf", tgzPath, "-C", tmp], { stdio: "inherit" });
  if (r.status !== 0) throw new Error(`tar extract failed (exit ${r.status})`);
  // npm tarball stores the binary at package/bin/<binName>
  const candidate = join(tmp, "package", "bin", binName);
  if (existsSync(candidate)) return candidate;
  // fallback: some platform tarballs omit the .exe suffix
  const fallback = join(tmp, "package", "bin", "tokscale");
  if (existsSync(fallback)) return fallback;
  throw new Error(`package/bin/${binName} missing in tarball`);
}

async function main() {
  const opts = parseOpts();
  const rsVersion = pinnedVersion();
  if (opts.version && opts.version !== rsVersion) {
    console.warn(`[fetch-tokscale] WARN: requested ${opts.version} but tokscale.rs pins ${rsVersion}; using .rs value`);
  }
  const version = opts.version && opts.version !== rsVersion ? rsVersion : (opts.version || rsVersion);

  const triple = npmTriple(opts.target);
  const win = triple.startsWith("win32-");
  const binName = win ? "tokscale.exe" : "tokscale";
  const outPath = join(OUT_DIR, binName);

  if (existsSync(outPath)) {
    console.log(`[fetch-tokscale] exists: ${outPath} (delete to re-fetch)`);
    return;
  }

  mkdirSync(OUT_DIR, { recursive: true });
  const url = `${opts.registry}/@tokscale/cli-${triple}/-/cli-${triple}-${version}.tgz`;
  console.log(`[fetch-tokscale] GET ${url}`);
  const tgz = join(tmpdir(), `tokscale-${triple}-${version}.tgz`);
  try {
    await download(url, tgz);
  } catch (e) {
    console.error(`[fetch-tokscale] download failed: ${e.message}`);
    console.error(`[fetch-tokscale] hint: set TOKSCALE_REGISTRY=https://registry.npmjs.org if the mirror fails`);
    process.exit(1);
  }

  const extracted = extractBinary(tgz, binName);
  if (!win) spawnSync("chmod", ["+x", extracted], { stdio: "inherit" });
  renameSync(extracted, outPath);
  rmSync(tgz, { force: true });
  // cleanup the temp extract dir (rename moved the file out)
  rmSync(join(tmpdir(), `tokscale-extract-${process.pid}`), { recursive: true, force: true });
  console.log(`[fetch-tokscale] installed: ${outPath}`);
}

main().catch((e) => {
  console.error(`[fetch-tokscale] ${e.message}`);
  process.exit(1);
});
