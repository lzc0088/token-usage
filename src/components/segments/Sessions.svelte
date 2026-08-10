<script lang="ts">
  import { api, type Currency, type SessionDetailRow, type SessionRoundVm, type SessionVm } from "../../lib/api";
  import { PALETTE } from "../../lib/constants";
  import { formatCost, splitTokens } from "../../lib/format";
  import { modelVendor } from "../../lib/meta/models";
  import { toolMeta } from "../../lib/meta/tools";
  import { listen } from "@tauri-apps/api/event";
  import { t } from "../../lib/i18n.svelte";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import { COLLECTION_UPDATED } from "../../lib/events";

  const DETAIL_CAPABLE_TOOLS = ["claude", "codex", "opencode"] as const;

  let { currency, cnyRate = 7.2 }: { currency: Currency; cnyRate?: number } = $props();

  let sessions = $state<SessionVm[] | null>(null);
  let loadAttempted = $state(false);
  let expanded = $state<string | null>(null);
  let detail = $state<SessionDetailRow[] | null>(null);

  let viewing = $state<SessionVm | null>(null);
  let viewRounds = $state<SessionRoundVm[] | null>(null);

  // Detail-page sort: "time" (newest first) | "token" (most tokens first).
  type RoundSort = "time" | "token";
  let roundSort = $state<RoundSort>("time");

  const sortedRounds = $derived.by(() => {
    // Rust already returns the most recent 300 rounds (time desc). The TOKEN
    // toggle just re-orders that set; time toggle keeps the Rust order.
    const arr = viewRounds ? [...viewRounds] : [];
    if (roundSort === "token") {
      arr.sort((a, b) => b.total_tokens - a.total_tokens);
    }
    return arr;
  });

  type SortKey = "token" | "latest" | "proj" | "tool";
  let sort = $state<SortKey>("latest");

  // Generation counter prevents stale async responses from overwriting fresher
  // data when the user rapidly clicks different sessions.
  let detailGen = 0;
  let roundGen = 0;

  function toggleExpand(tool: string, sid: string): void {
    const key = `${tool}:${sid}`;
    if (expanded === key) { expanded = null; detail = null; return; }
    expanded = key;
    detail = null;
    const gen = ++detailGen;
    api.getSessionDetail(tool, sid)
      .then(d => { if (gen === detailGen) detail = d; })
      .catch(() => { if (gen === detailGen) detail = null; });
  }

  function openDetail(s: SessionVm, e: MouseEvent): void {
    e.stopPropagation();
    viewing = s;
    viewRounds = null;
    const gen = ++roundGen;
    api.getSessionRounds(s.tool, s.session_id)
      .then(d => { if (gen === roundGen) viewRounds = d; })
      .catch(() => { if (gen === roundGen) viewRounds = null; });
  }

  function closeDetail(): void { viewing = null; viewRounds = null; }

  $effect(() => {
    let cancelled = false;
    const fetch = async () => {
      try {
        const s = await api.getSessions();
        if (!cancelled) { sessions = s; loadAttempted = true; }
      } catch { if (!cancelled) loadAttempted = true; }
    };
    fetch();
    const un = listen<void>(COLLECTION_UPDATED, () => { fetch(); });
    return () => {
      cancelled = true;
      un.then(fn => fn());
    };
  });

  const sorted = $derived.by(() => {
    const arr = sessions ? [...sessions] : [];
    arr.sort((a, b) => {
      if (sort === "latest") {
        const at = a.last_used_at ?? "9999-99-99 99:99";
        const bt = b.last_used_at ?? "9999-99-99 99:99";
        return bt.localeCompare(at);
      }
      if (sort === "proj") return (projectLabel(a)).localeCompare(projectLabel(b));
      if (sort === "tool") return a.tool.localeCompare(b.tool) || b.tokens - a.tokens;
      return b.tokens - a.tokens;
    });
    return arr;
  });

  function modelLabel(count: number, models: string): { tag: string; isCount: boolean; color?: string } {
    if (count <= 1) {
      const first = models.split(",").filter(Boolean)[0] || models;
      const mv = modelVendor(first);
      return { tag: first || "—", isCount: false, color: mv?.color };
    }
    return { tag: `${count}${t("sessions.modelsCount")}`, isCount: true };
  }

  function projectLabel(s: SessionVm): string {
    return s.project_name ?? toolMeta(s.tool).label;
  }

  const palette = PALETTE;

  function composeDetail(dr: SessionDetailRow): { label: string; tokens: number; pct: number; color: string }[] {
    const total = dr.tokens || 1;
    return [
      { label: t("detail.input"), tokens: dr.input, pct: (dr.input / total) * 100, color: palette[0] },
      { label: t("detail.output"), tokens: dr.output, pct: (dr.output / total) * 100, color: palette[1] },
      { label: t("detail.cache"), tokens: dr.cache_read, pct: (dr.cache_read / total) * 100, color: palette[2] },
    ];
  }
</script>

<div class="seg-body">
  {#if viewing}
    <!-- ── session detail page ── -->
    <div class="view-header">
      <button type="button" class="back-btn" onclick={closeDetail} aria-label="返回">←</button>
      <span class="view-title">{t("sessions.detail")}<span class="rounds-count">{viewRounds?.length ?? 0}</span></span>
      <div class="rd-sort">
        {#each [["time", t("sessions.sortTime")], ["token", t("sessions.sortToken")]] as [k, label] (k)}
          <button class:on={roundSort === (k as RoundSort)} aria-pressed={roundSort === (k as RoundSort)} onclick={() => (roundSort = k as RoundSort)}>{label}</button>
        {/each}
      </div>
    </div>
    <!-- round list (one row per user input) -->
    <div class="view-body">
      {#if viewRounds === null}
        <Skeleton type="list" rows={2} />
      {:else if viewRounds.length === 0}
        <EmptyState title={t("sessions.noRounds")} />
      {:else}
        {#each sortedRounds as r, ri (ri)}
          {@const ts = splitTokens(r.total_tokens)}
          <div class="rd-row">
            <div class="rd-main">
              <div class="rd-line1">
                <span class="rd-user">👤</span>
                <span class="rd-text">{r.user_text || t("sessions.noText")}</span>
              </div>
              <div class="rd-line2">
                <span class="rd-time">{r.timestamp ?? ""}</span>
                <span class="rd-sep">·</span>
                <span class="rd-turns">{r.turns} turns</span>
                {#if r.tools > 0}
                  <span class="rd-sep">·</span>
                  <span class="rd-tools">{r.tools} tools</span>
                {/if}
                {#if r.model}
                  <span class="rd-sep">·</span>
                  <span class="rd-model">{r.model}</span>
                {/if}
              </div>
            </div>
            <div class="rd-right">
              <span class="rd-cost">{formatCost(r.cost_usd, currency, cnyRate)}</span>
              <span class="rd-tokens">{ts.value}<span class="tku">{ts.unit}</span></span>
            </div>
          </div>
        {/each}
      {/if}
    </div>
  {:else}
    <!-- ── session list ── -->
    <div class="bd-header">
      <span class="bd-title">{t("sessions.history")}<span class="bd-count">{sorted.length}</span></span>
      <div class="bd-sort">
        {#each [["latest", t("sessions.sortLatest")], ["token", t("sessions.sortToken")], ["proj", t("sessions.sortProj")], ["tool", t("sessions.sortTool")]] as [k, label] (k)}
          <button class:on={sort === (k as SortKey)} aria-pressed={sort === (k as SortKey)} onclick={() => (sort = k as SortKey)}>{label}</button>
        {/each}
      </div>
    </div>

    {#if sessions === null && !loadAttempted}
      <Skeleton type="list" rows={2} />
    {:else if sessions === null}
      <EmptyState title={t("common.loadFailed")} />
    {:else if sessions.length === 0}
      <EmptyState title={t("sessions.empty")} />
    {:else}
      {#each sorted as s (s.tool + s.session_id)}
        {@const st = splitTokens(s.tokens)}
        {@const open = expanded === `${s.tool}:${s.session_id}`}
        {@const ml = modelLabel(s.model_count, s.models)}
        {@const tc = toolMeta(s.tool).color}
        <div
          class="srow"
          role="button"
          tabindex="0"
          onclick={() => toggleExpand(s.tool, s.session_id)}
          onkeydown={(e: KeyboardEvent) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); toggleExpand(s.tool, s.session_id); } }}
        >
          <div class="s-main">
            <div class="s-line s-l1"><span class="s-proj">{projectLabel(s)}</span></div>
            <div class="s-line s-l2"><span class="s-id">{s.session_id}</span></div>
            <div class="s-line s-l3">
              <ToolIcon tool={s.tool} badge={false} size={9} />
              <span class="s-tool-tag model-colored" style="color:{tc};background:{tc}18;border-color:{tc}33">{toolMeta(s.tool).label}</span>
              {#if ml.color && !ml.isCount}
                <span class="s-model-tag model-colored" style="color:{ml.color};background:{ml.color}18;border-color:{ml.color}33">{ml.tag}</span>
              {:else}
                <span class="s-model-tag" class:tag-count={ml.isCount}>{ml.tag}</span>
              {/if}
            </div>
            <div class="s-line s-l4">
              <span class="s-time">{s.last_used_at ?? "—"}</span>
              <span class="s-msgs">{s.messages}{t("breakdown.msgs")}</span>
            </div>
          </div>
          <div class="s-right">
            <div class="s-meta">
              <span class="s-cost">{formatCost(s.cost_usd, currency, cnyRate)}</span>
              <span class="s-tokens">{st.value}<span class="tku">{st.unit}</span></span>
            </div>
            {#if (DETAIL_CAPABLE_TOOLS as readonly string[]).includes(s.tool)}
              <button
                class="s-arr"
                title={t("sessions.viewDetail")}
                aria-label={t("sessions.viewDetail")}
                onclick={(e: MouseEvent) => openDetail(s, e)}
              >→</button>
            {/if}
          </div>
        </div>
        {#if open}
          <div class="s-detail">
            {#if detail === null}
              <Skeleton type="list" rows={2} />
            {:else if detail.length === 0}
              <EmptyState title={t("sessions.noDetail")} compact />
            {:else}
              {#each detail as dr (dr.model)}
                {@const dm = toolMeta(dr.model)}
                <div class="det-entry">
                  <div class="det-model-row">
                    <ToolIcon tool={dr.model} badge={false} size={11} />
                    <span class="det-model-name">{dm.label}</span>
                    <span class="det-model-tokens">{splitTokens(dr.tokens).value}<span class="tku">{splitTokens(dr.tokens).unit}</span></span>
                    <span class="det-model-cost">{formatCost(dr.cost_usd, currency, cnyRate)}</span>
                  </div>
                  {#each composeDetail(dr) as cd (`${dr.model}-${cd.label}`)}
                    {@const cds = splitTokens(cd.tokens)}
                    <div class="det-comp-row">
                      <span class="det-bar-label"><span class="det-dot" style="background:{cd.color}"></span>{cd.label}</span>
                      <div class="det-bar"><i style="width:{Math.max(2, cd.pct).toFixed(1)}%;background:{cd.color}"></i></div>
                      <span class="det-pct">{cd.pct.toFixed(1)}<span class="pct-u">%</span></span>
                      <span class="det-tok">{cds.value}<span class="tku">{cds.unit}</span></span>
                    </div>
                  {/each}
                </div>
              {/each}
            {/if}
          </div>
        {/if}
      {/each}
    {/if}
  {/if}
</div>

<style>
  .seg-body { display: flex; flex-direction: column; }

  .bd-header { display: flex; align-items: center; justify-content: space-between; padding: 10px 16px 12px; }
  .bd-title { font-size: 13px; color: var(--text-dim); display: flex; align-items: center; gap: 7px; }
  .bd-count { font-family: var(--font-mono); font-size: 11px; font-weight: 600; color: var(--amber); background: var(--amber-bg); padding: 1px 8px; border-radius: 10px; line-height: 1.4; }
  .bd-sort { display: inline-flex; gap: 1px; background: var(--glass-3); border-radius: 8px; padding: 2px; }
  .bd-sort button { background: transparent; border: none; color: var(--text-faint); font-family: var(--font-ui); font-size: 11px; font-weight: 600; padding: 4px 10px; border-radius: 6px; cursor: pointer; }
  .bd-sort button:hover { color: var(--text-dim); }
  .bd-sort button.on { background: var(--amber); color: var(--badge-text); }

  /* ── list row ── */
  .srow { display: grid; grid-template-columns: 1fr auto; align-items: start; gap: 10px; padding: 10px 16px; cursor: pointer; background: none; border: none; font-family: inherit; text-align: left; width: 100%; border-bottom: 1px dashed var(--border-dim); }
  .srow:hover { background: rgba(232,176,75,.04); }
  .s-main { min-width: 0; display: flex; flex-direction: column; gap: 2px; }
  .s-line { display: flex; align-items: baseline; gap: 6px; }
  .s-l1 .s-proj { font-size: 13px; color: var(--text); font-weight: 600; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .s-l2 .s-id { font-family: var(--font-mono); font-size: 10px; color: var(--text-faint); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
  .s-l3 { display: flex; align-items: center; gap: 4px; }
  .s-tool-tag { font-size: 10px; white-space: nowrap; }
  .s-tool-tag.model-colored { padding: 0px 5px; border-radius: 3px; line-height: 1.6; border: 1px solid; }
  .s-model-tag { font-size: 10px; color: var(--text-dim); white-space: nowrap; }
  .s-model-tag.model-colored { padding: 0px 5px; border-radius: 3px; line-height: 1.6; border: 1px solid; }
  .s-model-tag.tag-count { color: var(--text-faint); background: var(--glass-3); padding: 0px 5px; border-radius: 3px; line-height: 1.6; }
  .s-l4 { font-size: 10px; color: var(--text-faint); gap: 10px; }
  .s-time { font-family: var(--font-mono); }

  /* right side */
  .s-right { display: flex; flex-direction: column; align-items: flex-end; justify-content: space-between; gap: 2px; flex-shrink: 0; min-height: 100%; padding-top: 4px; }
  .s-meta { display: flex; flex-direction: column; align-items: flex-end; gap: 1px; }
  .s-cost { font-size: 11px; color: var(--amber); user-select: text; -webkit-user-select: text; }
  .s-tokens { font-family: var(--font-mono); font-size: 12px; color: var(--text-dim); user-select: text; -webkit-user-select: text; }
  .tku { font-size: 8px; color: var(--text-faint); margin-left: 2px; font-weight: 600; }
  .s-arr {
    background: none; border: none; color: var(--text-faint);
    font-size: 18px; line-height: 1; cursor: pointer;
    padding: 2px 4px; font-family: var(--font-mono); flex-shrink: 0;
    transition: color .15s;
  }
  .s-arr:hover { color: var(--amber); }

  /* ── inline expand ── */
  .s-detail { padding: 8px 24px 10px 24px; border-bottom: 1px dashed var(--border-dim); background: var(--overlay-dark); }
  .det-entry { margin-bottom: 8px; padding-bottom: 8px; border-bottom: 1px solid var(--border-dim); }
  .det-entry:last-child { margin-bottom: 0; padding-bottom: 0; border-bottom: none; }
  .det-model-row { display: flex; align-items: center; gap: 3px; padding: 2px 0; }
  .det-model-name { font-size: 12px; color: var(--text); flex: 1; }
  .det-model-tokens { font-family: var(--font-mono); font-size: 10px; color: var(--text-dim); margin-right: 8px; }
  .det-model-cost { font-family: var(--font-mono); font-size: 10px; color: var(--amber); }
  .det-comp-row { display: grid; grid-template-columns: 1fr 100px 50px 56px; align-items: center; gap: 8px; padding: 2px 0; }
  .det-bar-label { font-size: 10px; color: var(--text-faint); text-align: left; display: flex; align-items: center; gap: 4px; }
  /* Per-segment overrides on the shared .det-* base (breakdown.css) */
  .det-dot { width: 9px; height: 9px; border-radius: 2px; flex-shrink: 0; }
  .det-bar { height: 3px; border-radius: 1.5px; }
  .det-bar i { border-radius: 1.5px; }
  .pct-u { font-size: 7px; margin-left: 1px; }

  /* ── detail page ── */
  .view-header { display: flex; align-items: center; gap: 8px; padding: 10px 16px; border-bottom: 1px solid var(--border-dim); }
  .view-title { font-size: 13px; color: var(--text-dim); flex: 1; display: flex; align-items: center; gap: 7px; }
  .rounds-count { font-family: var(--font-mono); font-size: 11px; font-weight: 600; color: var(--amber); background: var(--amber-bg); padding: 1px 8px; border-radius: 10px; line-height: 1.4; }
  .rd-sort { display: inline-flex; gap: 1px; background: var(--glass-3); border-radius: 8px; padding: 2px; }
  .rd-sort button { background: transparent; border: none; color: var(--text-faint); font-family: var(--font-ui); font-size: 11px; font-weight: 600; padding: 4px 10px; border-radius: 6px; cursor: pointer; }
  .rd-sort button:hover { color: var(--text-dim); }
  .rd-sort button.on { background: var(--amber); color: var(--badge-text); }
  .back-btn { background: none; border: none; color: var(--amber); font-size: 16px; cursor: pointer; padding: 2px 4px; line-height: 1; }
  .back-btn:hover { color: var(--text); }
  .view-body { flex: 1; }
  .rd-row { display: grid; grid-template-columns: 1fr auto; align-items: center; gap: 10px; padding: 9px 16px; border-bottom: 1px dashed var(--border-dim); }
  .rd-row:hover { background: rgba(232,176,75,.04); }
  .rd-main { min-width: 0; display: flex; flex-direction: column; gap: 3px; }
  .rd-line1 { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
  .rd-user { font-size: 12px; flex-shrink: 0; }
  .rd-text { font-size: 12px; color: var(--text); white-space: nowrap; overflow: hidden; text-overflow: ellipsis; flex: 1; }
  .rd-line2 { display: flex; align-items: baseline; gap: 6px; font-size: 11px; color: var(--text-faint); }
  .rd-time { font-family: var(--font-mono); }
  .rd-sep { color: var(--text-faint); }
  .rd-turns { color: var(--text-dim); }
  .rd-tools { color: var(--lime); }
  .rd-model { color: var(--violet); font-size: 10px; }
  .rd-right { display: flex; flex-direction: column; align-items: flex-end; gap: 1px; flex-shrink: 0; }
  .rd-cost { font-size: 11px; color: var(--amber); font-family: var(--font-mono); }
  .rd-tokens { font-family: var(--font-mono); font-size: 12px; color: var(--text-dim); }
</style>
