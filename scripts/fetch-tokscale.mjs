// Fetches the @tokscale/cli-{triple} native binary for the current platform
// (or an explicit --target) and places it at src-tauri/bin/tokscale[.exe],
// ready for Tauri's bundle.resources to pick up.
//
// Env:
//   TOKSCALE_REGISTRY  override npm registry (default: https://registry.npmmirror.com)
//   TOKSCALE_VERSION   override version (default: read from tokscale.rs)
//   TAURI_TARGET       rust target triple, e.g. aarch64-apple-darwin (overrides host)
// Argv:
//   --target=<triple>  same as TAURI_TARGET
//   --registry=<url>   same as TOKSCALE_REGISTRY
//   --version=<x.y.z>  same as TOKSCALE_VERSION
//   --latest           query npm for the latest version (ignores pinned version)
//
// Without --latest: idempotent — skips if the output already exists.
// With --latest: always re-fetches and writes the resolved version back to
// tokscale.rs so the Rust runtime install fallback stays in sync.

import { copyFileSync, createWriteStream, existsSync, mkdirSync, readFileSync, rmSync, renameSync, writeFileSync } from "node:fs";
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
    latest: false,
  };
  for (const a of process.argv.slice(2)) {
    if (a === "--latest") { opts.latest = true; continue; }
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

// Write a new version back to tokscale.rs (in-place string replace).
function writePinnedVersion(version) {
  let src = readFileSync(RS_PATH, "utf8");
  const re = /pub const TOKSCALE_VERSION:\s*&str\s*=\s*"([^"]+)"/;
  if (!re.test(src)) throw new Error(`cannot find TOKSCALE_VERSION in ${RS_PATH}`);
  src = src.replace(re, `pub const TOKSCALE_VERSION: &str = "${version}"`);
  writeFileSync(RS_PATH, src, "utf8");
  console.log(`[fetch-tokscale] updated tokscale.rs TOKSCALE_VERSION → ${version}`);
}

// Query npm registry for the latest version of @tokscale/cli-{triple}.
async function fetchLatestVersion(registry, triple) {
  const url = `${registry}/@tokscale/cli-${triple}/latest`;
  console.log(`[fetch-tokscale] query latest: ${url}`);
  const body = await getJson(registry, `/@tokscale/cli-${triple}/latest`);
  const v = body?.version;
  if (!v) throw new Error(`could not resolve latest version for @tokscale/cli-${triple}`);
  return v;
}

// HTTPS GET returning parsed JSON (handles redirects).
function getJson(registryBase, path) {
  const url = registryBase + path;
  return new Promise((resolve, reject) => {
    https.get(url, (res) => {
      if (res.statusCode >= 300 && res.statusCode < 400 && res.headers.location) {
        res.resume();
        // Follow redirect (may be absolute or relative)
        const loc = res.headers.location;
        const nextUrl = loc.startsWith("http") ? loc : new URL(loc, registryBase).href;
        https.get(nextUrl, (r2) => {
          let data = "";
          r2.on("data", (c) => (data += c));
          r2.on("end", () => { try { resolve(JSON.parse(data)); } catch (e) { reject(e); } });
        }).on("error", reject);
        return;
      }
      if (res.statusCode !== 200) {
        res.resume();
        return reject(new Error(`HTTP ${res.statusCode} for ${url}`));
      }
      let data = "";
      res.on("data", (c) => (data += c));
      res.on("end", () => { try { resolve(JSON.parse(data)); } catch (e) { reject(e); } });
    }).on("error", reject);
  });
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

// Use the system tar (mac/linux + Win10+) to extract the tarball. The temp dir
// is created under `workDir` (same volume as the final destination) so the
// subsequent move doesn't hit EXDEV on cross-volume Windows runners
// (C: temp → D: workspace).
function extractBinary(tgzPath, binName, workDir) {
  const tmp = join(workDir, `.tokscale-extract-${process.pid}`);
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

// Move a file, falling back to copy+unlink when rename fails with EXDEV
// (cross-device). On same-volume moves rename is atomic and instant.
function moveSync(src, dest) {
  try {
    renameSync(src, dest);
  } catch (e) {
    if (e.code === "EXDEV") {
      copyFileSync(src, dest);
      rmSync(src, { force: true });
    } else {
      throw e;
    }
  }
}

async function main() {
  const opts = parseOpts();

  const triple = npmTriple(opts.target);
  const win = triple.startsWith("win32-");
  const binName = win ? "tokscale.exe" : "tokscale";
  const outPath = join(OUT_DIR, binName);

  // ── resolve version ──────────────────────────────────────────────────────
  let version;
  if (opts.latest) {
    // Query npm for the latest published version. Fall back to the pinned
    // constant on any network or parse error so CI/dev still works offline.
    try {
      version = await fetchLatestVersion(opts.registry, triple);
      // Delete stale binary so we always re-download on --latest.
      rmSync(outPath, { force: true });
    } catch (e) {
      console.warn(`[fetch-tokscale] latest check failed: ${e.message}; falling back to pinned version`);
      version = pinnedVersion();
    }
  } else {
    version = opts.version || pinnedVersion();
  }

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

  const extracted = extractBinary(tgz, binName, OUT_DIR);
  if (!win) spawnSync("chmod", ["+x", extracted], { stdio: "inherit" });
  moveSync(extracted, outPath);
  rmSync(tgz, { force: true });
  // cleanup the temp extract dir under OUT_DIR (move copied the file out)
  rmSync(join(OUT_DIR, `.tokscale-extract-${process.pid}`), { recursive: true, force: true });
  console.log(`[fetch-tokscale] installed: ${outPath} (v${version})`);

  // ── sync Rust constant ───────────────────────────────────────────────────
  if (opts.latest) {
    writePinnedVersion(version);
  }
}

main().catch((e) => {
  console.error(`[fetch-tokscale] ${e.message}`);
  process.exit(1);
});
