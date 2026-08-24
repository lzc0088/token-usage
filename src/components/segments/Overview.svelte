<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "../../lib/i18n.svelte";
  import { api, type Breakdown, type Currency, type Quota, type Summary } from "../../lib/api";
  import { QUOTA_UPDATED } from "../../lib/events";
  import { PALETTE } from "../../lib/constants";
  import { formatCost, splitTokens } from "../../lib/format";
  import { modelVendor } from "../../lib/meta/models";
  import { toolMeta } from "../../lib/meta/tools";
  import QuotaCard from "../common/QuotaCard.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";
  import { periodValue } from "../../stores/period.svelte";
  import { setSegment } from "../../stores/segment.svelte";

  interface OverviewConfig {
    layout_overview_sub?: string[] | null;
    overview_quota_vendors?: string[] | null;
    quota_active_vendors?: string[] | null;
    quota_progress_mode?: string;
  }

  let {
    summary,
    currency,
    cnyRate = 7.2,
    config,
  }: { summary: Summary | null; currency: Currency; cnyRate?: number; config?: OverviewConfig } = $props();

  let toolB = $state<Breakdown | null>(null);
  let modelB = $state<Breakdown | null>(null);
  let quotas = $state<Quota[]>([]);
  let nowMs = $state(Date.now());

  // Derive quota filter/display state from the parent config prop so it stays
  // in sync with App.svelte's live reload (no separate api.getConfig() needed).
  async function openSettingsTo(partition: string): Promise<void> {
    try { await invoke("open_settings", { target: partition }); } catch {}
  }

  let overviewQuotaVendors = $derived(config?.overview_quota_vendors ?? null);
  let activeVendors = $derived(config?.quota_active_vendors ?? null);
  let progressMode = $derived((config?.quota_progress_mode as "用量" | "剩余") ?? "剩余");

  // Generation counter prevents a slow initial fetch from overwriting fresher
  // quota data delivered by a later `quota:updated` event re-fetch.
  let loadGen = 0;

  $effect(() => {
    const p = periodValue();
    let cancelled = false;
    const myGen = ++loadGen;
    (async () => {
      try {
        const [t, m, q] = await Promise.all([
          api.getBreakdown(p, "tool"),
          api.getBreakdown(p, "model"),
          api.getQuotas(),
        ]);
        if (cancelled || myGen !== loadGen) return;
        toolB = t;
        modelB = m;
        quotas = q;
      } catch {
        if (cancelled || myGen !== loadGen) return;
        toolB = null;
        modelB = null;
        quotas = [];
      }
    })();
    return () => { cancelled = true; };
  });
  $effect(() => {
    const t = setInterval(() => { nowMs = Date.now(); }, 30_000);
    return () => clearInterval(t);
  });

  // Live-update quotas when the background scheduler finishes a refresh cycle.
  // Debounce: the scheduler may emit several quota:updated events in quick
  // succession (one per vendor in a batch), so coalesce them into one re-fetch.
  let quotaGen = 0;
  $effect(() => {
    let t: number | undefined;
    const doReload = () => {
      if (t !== undefined) clearTimeout(t);
      t = window.setTimeout(async () => {
        const myGen = ++quotaGen;
        try {
          const q = await api.getQuotas();
          if (myGen !== quotaGen) return;
          quotas = q;
        } catch {}
        t = undefined;
      }, 200);
    };
    const un_quota = listen<void>(QUOTA_UPDATED, doReload);
    // Reload when the popover becomes visible again (e.g. returning from the
    // settings window after clearing a credential). macOS suspends a hidden
    // webview, so the `quota:updated` event fired from settings can be dropped
    // — visibilitychange fires reliably on show and picks up the fresh cache.
    const onVis = () => { if (document.visibilityState === "visible") doReload(); };
    document.addEventListener("visibilitychange", onVis);
    return () => {
      if (t !== undefined) clearTimeout(t);
      document.removeEventListener("visibilitychange", onVis);
      un_quota.then((un) => un());
    };
  });

  // On mount: if cached quota is older than the configured cadence
  // (quota_refresh_interval), trigger a background refresh now so the user
  // never sees stale data on entry. No-op when fresh — the scheduler handles
  // it. The quota:updated listener above reloads once the refresh finishes.
  $effect(() => {
    void api.refreshQuotasIfStale().catch(() => {});
  });

  // Exposed for the preview-config sync $effect below.
  let layoutSig = $derived(JSON.stringify(overviewQuotaVendors) + "|" + JSON.stringify(activeVendors));
  const visibleQuotas = $derived.by(() => {
    // Pin the layout signature so this block re-runs when MainView config
    // changes — see the $effect below for the complementary quota reload.
    void layoutSig;
    if (quotas.length === 0) return [];
    let filtered = quotas;
    if (activeVendors != null) {
      const set = new Set(activeVendors);
      filtered = filtered.filter(q => set.has(q.vendor));
    }
    if (overviewQuotaVendors != null) {
      const set = new Set(overviewQuotaVendors);
      filtered = filtered.filter(q => set.has(q.vendor));
      const order = new Map(overviewQuotaVendors.map((v, i) => [v, i]));
      filtered = [...filtered].sort((a, b) => (order.get(a.vendor) ?? 999) - (order.get(b.vendor) ?? 999));
    }
    return filtered;
  });

  const DEFAULT_SUB_ORDER = ["overview_io", "overview_tools", "overview_models", "overview_quotas"] as const;
  // $derived so section order/visibility tracks config prop changes live
  // (not just on component remount).
  const subOrder = $derived(config?.layout_overview_sub ?? DEFAULT_SUB_ORDER);

  // All sections are shown by default (visibility controlled by subOrder)
  // The sections themselves handle empty state internally.

  const palette = PALETTE;

</script>
<div class="ov-body">
  {#if !summary && !toolB && !modelB && quotas.length === 0}
    <Skeleton type="overview" rows={2} />
  {:else if subOrder.length === 0}
    <EmptyState title={t("overview.noModules")} hint={t("overview.noModulesHint")} actionLabel={t("common.goSettings")} onAction={() => openSettingsTo("mainview")} />
  {:else}
  {#each subOrder as sectionKey (sectionKey)}
    {#if sectionKey === "overview_io"}
      <section class="module">
        <div class="split2">
          {#if summary}
            {@const cells = [
              { k: t("detail.input"), v: summary.input, cls: "tok-in" },
              { k: t("detail.output"), v: summary.output, cls: "tok-out" },
              { k: t("detail.cacheRead"), v: summary.cache_read, cls: "tok-cr" },
              { k: t("detail.cacheWrite"), v: summary.cache_write, cls: "tok-cw" },
            ]}
            {#each cells as cell}
              {@const s = splitTokens(cell.v)}
              <div class="scell"><div class="k">{cell.k}</div><div class="v {cell.cls}">{s.value}<span class="u">{s.unit}</span></div></div>
            {/each}
          {:else}
            <EmptyState title={t("overview.noData")} compact />
          {/if}
        </div>
      </section>
    {/if}

    {#if sectionKey === "overview_tools"}
      <section class="module">
        <div class="sec-h"><span><span class="title-dot" style="background:var(--amber)"></span>{t("overview.tools")}</span><button type="button" class="more" onclick={() => setSegment("tools")}>{t("overview.all")}</button></div>
        {#if toolB && toolB.entries.length > 0}
          {#each toolB.entries.slice(0, 3) as e, i (e.key)}
            {@const s = splitTokens(e.tokens)}
            {@const meta = toolMeta(e.key)}
            <button type="button" class="crow" onclick={() => setSegment("tools")}>
              <span class="rk" style="background:{meta.color};color:var(--badge-text)">{@html meta.icon}</span>
              <span class="nm">{meta.label}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)} / {s.value}<span class="su">{s.unit}</span></span></span>
              <div class="br"><i style="width:{Math.max(2, e.token_pct).toFixed(1)}%;background:{palette[i % palette.length]}"></i></div>
              <span class="pct-label">{e.token_pct.toFixed(1)}<span class="pct-unit">%</span></span>
              <span class="vl">{s.value}<span class="vlu">{s.unit}</span></span>
            </button>
          {/each}
        {:else}
          <EmptyState title={t("overview.noToolData")} compact />
        {/if}
      </section>
    {/if}

    {#if sectionKey === "overview_models"}
      <section class="module">
        <div class="sec-h"><span><span class="title-dot" style="background:var(--cyan)"></span>{t("overview.models")}</span><button type="button" class="more" onclick={() => setSegment("models")}>{t("overview.all")}</button></div>
        {#if modelB && modelB.entries.length > 0}
          {#each modelB.entries.slice(0, 3) as e, i (e.key)}
            {@const s = splitTokens(e.tokens)}
            {@const mv = modelVendor(e.key)}
            {@const meta = toolMeta(e.key)}
            <button type="button" class="crow" onclick={() => setSegment("models")}>
              <span class="rk" style="background:{meta.color};color:var(--badge-text)">{@html meta.icon}</span>
              <span class="nm">{meta.label}{#if mv}<span class="mvendor" style="color:{mv.color}">{mv.vendor}</span>{/if}<span class="sub">{formatCost(e.cost_usd, currency, cnyRate)} / {e.messages}{t("overview.msgs")}</span></span>
              <div class="br"><i style="width:{Math.max(2, e.token_pct).toFixed(1)}%;background:{palette[(i + 2) % palette.length]}"></i></div>
              <span class="pct-label">{e.token_pct.toFixed(1)}<span class="pct-unit">%</span></span>
              <span class="vl">{s.value}<span class="vlu">{s.unit}</span></span>
            </button>
          {/each}
        {:else}
          <EmptyState title={t("overview.noModelData")} compact />
        {/if}
      </section>
    {/if}

    {#if sectionKey === "overview_quotas"}
      <section class="module">
        <div class="sec-h"><span><span class="title-dot" style="background:var(--violet)"></span>{t("overview.quota")}</span><button type="button" class="more" onclick={() => setSegment("limit")}>{t("overview.all")}</button></div>
        {#if visibleQuotas.length > 0}
          <div class="qcard-list">
            {#each visibleQuotas as q (q.vendor)}
              <QuotaCard quota={q} {progressMode} {nowMs} {currency} {cnyRate} />
            {/each}
          </div>
        {:else}
          {#if quotas.length === 0}
            <EmptyState title={t("overview.emptyQuotaNoBinding")} hint={t("overview.emptyQuotaNoBindingHint")} actionLabel={t("common.goSettings")} onAction={() => openSettingsTo("account")} compact />
          {:else if overviewQuotaVendors !== null && overviewQuotaVendors.length === 0}
            <EmptyState title={t("overview.emptyNoneSelected")} hint={t("overview.emptyNoneSelectedHint")} actionLabel={t("common.goSettings")} onAction={() => openSettingsTo("mainview")} compact />
          {:else}
            <EmptyState title={t("overview.emptyAllDisabled")} hint={t("overview.emptyAllDisabledHint")} actionLabel={t("common.goSettings")} onAction={() => openSettingsTo("account")} compact />
          {/if}
        {/if}
      </section>
    {/if}
  {/each}
  {/if}
</div>

<style>
  .ov-body { padding: 14px 18px; display: flex; flex-direction: column; gap: 12px; }
  .module {
    padding: 10px 12px;
    background: var(--surface-tint);
    border: 1px solid var(--border-dim);
    border-radius: 10px;
  }
  .sec-h {
    font-size: 0.8667rem; font-weight: 700;
    color: var(--text);
    margin-bottom: 10px;
    display: flex; justify-content: space-between; align-items: center;
  }
  .sec-h .title-dot {
    display: inline-block; width: 8px; height: 8px; border-radius: 50%;
    margin-right: 6px; flex-shrink: 0;
  }
  .sec-h .more {
    color: var(--amber); cursor: pointer; font-size: 0.8667rem; font-weight: 600;
    background: none; border: none; padding: 0; font-family: inherit;
  }
  .split2 { display: grid; grid-template-columns: 1fr 1fr; gap: 8px; }
  .scell {
    background: var(--surface-tint); border: 1px solid var(--border-dim);
    border-radius: 9px; padding: 10px 11px;
  }
  .scell .k { font-size: 0.7333rem; color: var(--text-faint); }
  .scell .v {
    font-size: 1.333rem; font-weight: 500; color: var(--text); margin-top: 2px;
    display: flex; align-items: baseline; gap: 2px;
  }
  .scell .v .u { font-size: 0.7333rem; color: var(--text-faint); font-weight: 600; }
  /* Token-category colors — shared semantics with detail-composition bars
   * (Sessions / BreakdownList) so each token class reads one color everywhere. */
  .v.tok-in { color: var(--tok-input) !important; }
  .v.tok-out { color: var(--tok-output) !important; }
  .v.tok-cr { color: var(--tok-cache-r) !important; }
  .v.tok-cw { color: var(--tok-cache-w) !important; }
  .crow {
    display: flex; align-items: center; gap: 8px;
    padding: 8px 0; cursor: pointer;
    background: none; border: none; font-family: inherit; text-align: left; width: 100%;
    border-bottom: 1px dashed var(--border-dim);
  }
  .crow:last-child { border-bottom: none; }
  .crow:hover .nm { color: var(--amber); }
  .crow .rk {
    width: 24px; height: 24px; border-radius: 6px;
    display: flex; align-items: center; justify-content: center;
    font-size: 0.7333rem; font-weight: 700; flex-shrink: 0;
    font-style: normal;
  }
  .crow .nm {
    font-size: 0.8667rem; flex: 1; color: var(--text); transition: .15s;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .crow .nm .sub {
    display: block; font-size: 0.7333rem; color: var(--text-faint); margin-top: 1px;
    white-space: nowrap; overflow: hidden; text-overflow: ellipsis;
  }
  .crow .nm .mvendor {
    font-size: 0.6667rem; font-weight: 600; margin-left: 6px;
    padding: 1px 5px; border-radius: 4px;
    background: var(--glass-3);
    border: 1px solid var(--border-dim);
  }
  .crow .su { font-size: 0.6rem; margin-left: 2px; }
  .crow .br {
    width: 62px; height: 4px; background: var(--bar-track);
    border-radius: 2px; overflow: hidden; flex-shrink: 0;
  }
  .crow .br i { display: block; height: 100%; }
  .crow .pct-label {
    font-family: var(--font-mono); font-size: 0.7333rem; color: var(--text-faint);
    width: 42px; text-align: right; flex-shrink: 0;
  }
  .crow .pct-unit { font-size: 0.4667rem; margin-left: 1px; }
  .crow .vl {
    font-size: 0.8rem; color: var(--text-dim); width: 50px; text-align: right; flex-shrink: 0;
  }
  .crow .vlu { font-size: 0.5333rem; color: var(--text-faint); font-weight: 600; margin-left: 2px; }
  .qcard-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
</style>
