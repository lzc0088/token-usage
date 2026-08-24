// Data export utilities — JSON + CSV serialization.
// Pure functions, no I/O. File writing goes through Tauri's fs API.

import type { Summary, Breakdown, Trends, SessionVm } from "./api";

// ── Types ──────────────────────────────────────────────────────────────────

export interface ExportSnapshot {
  period: string;
  summary: Summary;
  breakdown_by_tool: Breakdown;
  breakdown_by_model: Breakdown;
}

export interface ExportPayload {
  generated_at: string;
  app: { name: string; version: string };
  snapshots: ExportSnapshot[];
  daily_trends: Trends;
  sessions: SessionVm[];
}

// ── CSV Helpers ────────────────────────────────────────────────────────────

const BOM = "﻿";

function csvEscape(value: unknown): string {
  const s = value === null || value === undefined ? "" : String(value);
  return /",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}

function toCsv(rows: Record<string, unknown>[], columns: string[]): string {
  const header = columns.map(csvEscape).join(",");
  const body = rows.map((row) =>
    columns.map((col) => csvEscape(row[col])).join(",")
  );
  return BOM + [header, ...body].join("\r\n") + "\r\n";
}

// ── Snapshot CSV ───────────────────────────────────────────────────────────

const SNAPSHOT_COLUMNS = [
  "period",
  "dimension",
  "name",
  "tokens",
  "cost_usd",
  "messages",
  "input",
  "output",
  "cache_read",
  "cache_write",
];

export function renderSnapshotCsv(snapshots: ExportSnapshot[]): string {
  const rows: Record<string, unknown>[] = [];
  for (const snap of snapshots) {
    // Tool breakdown
    for (const entry of snap.breakdown_by_tool.entries) {
      rows.push({
        period: snap.period,
        dimension: "tool",
        name: entry.key,
        tokens: entry.tokens,
        cost_usd: entry.cost_usd,
        messages: entry.messages,
        input: entry.input,
        output: entry.output,
        cache_read: entry.cache_read,
        cache_write: entry.cache_write,
      });
    }
    // Model breakdown
    for (const entry of snap.breakdown_by_model.entries) {
      rows.push({
        period: snap.period,
        dimension: "model",
        name: entry.key,
        tokens: entry.tokens,
        cost_usd: entry.cost_usd,
        messages: entry.messages,
        input: entry.input,
        output: entry.output,
        cache_read: entry.cache_read,
        cache_write: entry.cache_write,
      });
    }
  }
  return toCsv(rows, SNAPSHOT_COLUMNS);
}

// ── Daily Trends CSV ───────────────────────────────────────────────────────

const DAILY_COLUMNS = ["date", "tokens", "cost_usd", "messages"];

export function renderDailyCsv(trends: Trends): string {
  const rows = trends.points.map((p) => ({
    date: p.date,
    tokens: p.tokens,
    cost_usd: p.cost_usd,
    messages: p.messages,
  }));
  return toCsv(rows, DAILY_COLUMNS);
}

// ── Sessions CSV ───────────────────────────────────────────────────────────

const SESSION_COLUMNS = [
  "tool",
  "session_id",
  "tokens",
  "cost_usd",
  "messages",
  "rounds",
  "model_count",
  "models",
  "last_used_at",
  "project_name",
];

export function renderSessionsCsv(sessions: SessionVm[]): string {
  const rows = sessions.map((s) => ({
    tool: s.tool,
    session_id: s.session_id,
    tokens: s.tokens,
    cost_usd: s.cost_usd,
    messages: s.messages,
    rounds: s.rounds,
    model_count: s.model_count,
    models: s.models,
    last_used_at: s.last_used_at ?? "",
    project_name: s.project_name ?? "",
  }));
  return toCsv(rows, SESSION_COLUMNS);
}

// ── Full JSON Export ───────────────────────────────────────────────────────

export function renderExportJson(payload: ExportPayload): string {
  return JSON.stringify(payload, null, 2) + "\n";
}
