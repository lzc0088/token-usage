<script lang="ts">
  // 状态 segment. Shows only tools that are actively collecting data
  // (采集中) AND 已安装 AND 显示 AND 追踪. Data sources:
  //   - getToolsStatus()       — live probe (spawns tokscale) for status/diagnostics
  //   - getCollectionHealth()  — persisted health KV (per-tool last_seen)
  // plus config (collection_visible / collection_tracked) for list filtering.
  // Row layout: large tool icon on the left, name + messages/last-seen center,
  // health chip on the right. No circular dots — states are conveyed by text
  // chips (color + label, never color alone).
  //
  // Refresh is event-driven (COLLECTION_UPDATED / COLLECTION_ERROR /
  // COLLECTION_HEALTH) with a 1s debounce so a collection-storm doesn't
  // repeatedly spawn tokscale. A manual "重新检测" button forces a fresh probe.
  import { listen } from "@tauri-apps/api/event";
  import { api, type ClientStatus, type CollectionHealth, type Config } from "../../lib/api";
  import { t, getLang } from "../../lib/i18n.svelte";
  import ToolIcon from "../ui/ToolIcon.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import { COLLECTION_UPDATED, COLLECTION_ERROR, COLLECTION_HEALTH } from "../../lib/events";

  let { config = null }: { config?: Config | null } = $props();

  const lang = $derived(getLang());

  let tools = $state<ClientStatus[] | null>(null);
  let health = $state<CollectionHealth | null>(null);
  let loadError = $state(false);
  /** Expanded diagnostics row key ("client"), null = all collapsed. */
  let expanded = $state<string | null>(null);
  /** Generation counter: prevents a slow IPC response from overwriting fresher
   *  data that arrived via a later refresh. */
  let refreshGen = 0;
  /** True while a MANUAL re-check is in flight (button spinner + disabled). */
  let checking = $state(false);

  async function refresh(opts: { manual?: boolean } = {}): Promise<void> {
    if (opts.manual) checking = true;
    const gen = ++refreshGen;
    try {
      const [t, h] = await Promise.all([api.getToolsStatus(), api.getCollectionHealth()]);
      if (gen !== refreshGen) return;
      tools = t;
      health = h;
      loadError = false;
    } catch {
      if (gen !== refreshGen) return;
      loadError = true;
    } finally {
      if (opts.manual) checking = false;
    }
  }

  // Initial load + event-driven refresh. Collection events are debounced:
  // the initial backfill fires one ingest per tool, so a burst would otherwise
  // spawn tokscale repeatedly.
  $effect(() => {
    void refresh();
    const onFocus = () => { void refresh(); };
    window.addEventListener("focus", onFocus);
    let debounceTimer: number | undefined;
    const debounced = () => {
      window.clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(() => { void refresh(); }, 1000);
    };
    const un1 = listen<void>(COLLECTION_UPDATED, debounced);
    const un2 = listen<void>(COLLECTION_ERROR, debounced);
    const un3 = listen<void>(COLLECTION_HEALTH, debounced);
    return () => {
      window.removeEventListener("focus", onFocus);
      window.clearTimeout(debounceTimer);
      un1.then((u) => u());
      un2.then((u) => u());
      un3.then((u) => u());
    };
  });

  // ── merged view model ─────────────────────────────────────────────────────
  // Overlays config flags (显示/追踪 — undefined = all tools on) onto the live
  // probe + persisted last-seen so each row answers: installed? shown? tracked?
  // when did it last produce data?
  interface StatusRow {
    client: string;
    label: string;
    status: "active";
    message_count: number;
    last_seen_ms: number | null;
    diagnostics?: { code: string; severity: string; message: string }[];
  }

  const rows = $derived.by((): StatusRow[] | null => {
    if (!tools) return null;
    // The list only shows tools that are 采集中 (active) AND 显示 AND 追踪:
    //   active  = live probe sees the tool and it has produced data
    //             (waiting/missing tools are hidden)
    //   visible = in config.collection_visible (absent list = all on)
    //   tracked = in config.collection_tracked (absent list = all on)
    const allClients = tools.map((tool) => tool.client);
    const trackedSet = new Set(config?.collection_tracked ?? allClients);
    const visibleSet = new Set(config?.collection_visible ?? allClients);
    const byClient = health?.clients ?? {};
    return tools
      .filter(
        (tool) =>
          tool.status === "active" &&
          trackedSet.has(tool.client) &&
          visibleSet.has(tool.client),
      )
      .map((tool) => ({
        client: tool.client,
        label: tool.label,
        status: "active" as const,
        message_count: tool.message_count,
        last_seen_ms: byClient[tool.client]?.last_seen_ms ?? null,
        diagnostics: tool.diagnostics,
      }));
  });

  /** Compact relative time ("3分钟" / "2小时" / "3天"). */
  function fmtAgo(ms: number): string {
    const secs = Math.max(0, Math.round(ms / 1000));
    if (secs < 60) return lang === "en" ? `${secs}s` : `${secs}秒`;
    const mins = Math.round(secs / 60);
    if (mins < 60) return lang === "en" ? `${mins}m` : `${mins}分钟`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return lang === "en" ? `${hours}h` : `${hours}小时`;
    const days = Math.round(hours / 24);
    return lang === "en" ? `${days}d` : `${days}天`;
  }

  // i18n with {n} interpolation (e.g. status.messages / status.lastSeen). The
  // placeholder may be a count (messages) or a pre-formatted duration/label.
  function tn(key: string, n: number | string): string {
    return t(key).replace("{n}", String(n));
  }

  function hasDiag(row: StatusRow): boolean {
    return !!row.diagnostics && row.diagnostics.length > 0;
  }

  function toggle(client: string): void {
    expanded = expanded === client ? null : client;
  }
</script>

<div class="seg-body">
  <!-- ── tool list: only 采集中 + 显示 + 追踪 tools; right side shows health ── -->
  {#if loadError && tools === null}
    <EmptyState title={t("status.scanFailed")} icon="off" />
  {:else if tools === null}
    <Skeleton type="status" rows={5} />
  {:else}
    <div class="tool-head">
      <div class="head-left">
        <span class="head-title">{t("status.heading")}</span>
        {#if rows !== null}
          <span class="head-count">{rows.length}</span>
        {/if}
      </div>
      <button
        class="recheck"
        onclick={() => void refresh({ manual: true })}
        disabled={checking}
        aria-busy={checking}
        data-testid="status-recheck"
      >
        {#if checking}
          <svg class="spin" viewBox="0 0 12 12" width="11" height="11" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round">
            <path d="M6 1.2 A4.8 4.8 0 0 1 10.8 6" />
          </svg>
          {t("status.checking")}
        {:else}
          {t("status.refresh")}
        {/if}
      </button>
    </div>
    {#if rows !== null && rows.length === 0}
      <div class="empty">{t("status.noActive")}</div>
    {:else}
      <div class="tool-list">
        {#each rows as row (row.client)}
          <div class="row" class:clickable={hasDiag(row)}>
            <button
              class="row-main"
              onclick={() => hasDiag(row) && toggle(row.client)}
              aria-expanded={expanded === row.client}
            >
              <span class="ric">
                <ToolIcon tool={row.client} badge={false} size={20} />
              </span>
              <span class="rinfo">
                <span class="rname">{row.label}</span>
                <span class="rmeta">
                  <span class="rcount">{tn("status.messages", row.message_count)}</span>
                  <span class="rseen">
                    {#if row.last_seen_ms === null}
                      <span class="dim">{t("status.never")}</span>
                    {:else}
                      {tn("status.lastSeen", fmtAgo(Date.now() - row.last_seen_ms))}
                    {/if}
                  </span>
                </span>
              </span>
              <span class="spacer"></span>
              <span class="rstatus">
                <span class="chip active">{t("status.active")}</span>
                {#if hasDiag(row)}
                  <span class="chev" class:open={expanded === row.client}>
                    <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" stroke-width="1.5">
                      <path d="M3 4.5 6 7.5 9 4.5" stroke-linecap="round" stroke-linejoin="round" />
                    </svg>
                  </span>
                {/if}
              </span>
            </button>
            {#if hasDiag(row) && expanded === row.client}
              <div class="diag">
                {#each row.diagnostics as d (d.code + d.message)}
                  <div class="diag-item">
                    <span class={`sev ${d.severity}`}></span>
                    <span class="diag-msg">{d.message}</span>
                  </div>
                {/each}
              </div>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
  {/if}
</div>

<style>
  .seg-body {
    padding: 12px 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }

  /* ── chips (text state labels — never color alone, never circular) ── */
  .chip {
    font-size: 0.7rem;
    font-weight: 500;
    padding: 1px 7px;
    border-radius: 5px;
    line-height: 1.4;
    background: var(--surface-tint);
    color: var(--text-faint);
    flex-shrink: 0;
    white-space: nowrap;
  }
  .chip.active { background: rgba(108, 199, 116, 0.12); color: var(--lime); }
  .dim { color: var(--text-faint); }

  /* ── tool list ── */
  .tool-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding-top: 2px;
  }
  .head-left {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .head-title {
    color: var(--text-dim);
    font-size: 0.8rem;
    font-weight: 600;
  }
  .head-count {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 0.68rem;
    font-weight: 500;
    color: var(--text-dim);
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    border-radius: 20px;
    padding: 0 7px;
    line-height: 1.5;
  }
  .recheck {
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    color: var(--text-dim);
    font-family: var(--font-ui);
    font-size: 0.72rem;
    font-weight: 500;
    cursor: pointer;
    padding: 3px 10px;
    border-radius: 6px;
    transition: all 0.15s;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-width: 64px;
    justify-content: center;
  }
  .recheck:hover:not(:disabled) {
    border-color: var(--amber);
    color: var(--amber);
  }
  .recheck:disabled {
    cursor: default;
    opacity: 0.75;
  }
  .spin {
    animation: status-spin 0.8s linear infinite;
  }
  @keyframes status-spin {
    to { transform: rotate(360deg); }
  }
  @media (prefers-reduced-motion: reduce) {
    .spin { animation-duration: 1.6s; }
  }

  .tool-list {
    display: flex;
    flex-direction: column;
  }
  .row {
    border-bottom: 1px dashed var(--border-dim);
    padding: 9px 0;
  }
  .row:last-child { border-bottom: none; }
  .row.clickable .row-main { cursor: pointer; }
  .row-main {
    width: 100%;
    display: flex;
    align-items: center;
    gap: 10px;
    background: none;
    border: none;
    padding: 0;
    font-family: var(--font-ui);
    color: var(--text);
    text-align: left;
  }

  /* large icon block on the left */
  .ric {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 38px;
    height: 38px;
    flex-shrink: 0;
  }

  .rinfo {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .rname {
    font-size: 0.85rem;
    font-weight: 500;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rmeta {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .rcount {
    color: var(--text-faint);
    font-size: 0.7rem;
    white-space: nowrap;
  }
  .rseen {
    color: var(--text-dim);
    font-size: 0.7rem;
    white-space: nowrap;
  }
  .spacer { flex: 1; }

  /* right-side health status (正常/等待) */
  .rstatus {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .empty {
    font-size: 0.75rem;
    color: var(--text-faint);
    padding: 14px 0;
    text-align: center;
  }

  .chev {
    color: var(--text-faint);
    display: inline-flex;
    transition: transform 0.15s;
  }
  .chev.open { transform: rotate(180deg); }

  .diag {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 6px 0 4px 48px;
  }
  .diag-item {
    display: flex;
    align-items: flex-start;
    gap: 6px;
    font-size: 0.72rem;
    color: var(--coral);
    background: var(--coral-bg-soft);
    border: 1px solid var(--coral-border);
    border-radius: 6px;
    padding: 5px 8px;
  }
  .sev {
    width: 6px;
    height: 6px;
    border-radius: 2px;
    background: var(--coral);
    margin-top: 3px;
    flex-shrink: 0;
  }
  .diag-msg { white-space: pre-wrap; word-break: break-word; }
</style>
