<script lang="ts">
  // Desktop widget card (window label "widget") — a compact always-current
  // summary pinned to the desktop: today's tokens + cost + live rate, the two
  // tightest quota windows, and a 7-day sparkline. Same design system as the
  // popover (amber gradient family, borderless elevation).
  import { listen } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { api, type Config, type Quota, type Summary, type Trends } from "../lib/api";
  import { TODAY_UPDATED, QUOTA_UPDATED, COLLECTION_UPDATED } from "../lib/events";
  import { setLang, t } from "../lib/i18n.svelte";
  import { splitTokensCN, splitCost, formatLiveRate } from "../lib/format";
  import { VENDOR_LABELS } from "../lib/meta/vendors";

  let summary = $state<Summary | null>(null);
  let quotas = $state<Quota[]>([]);
  let trends = $state<Trends | null>(null);
  let config = $state<Config>({ currency: "both" });
  let loadAttempted = $state(false);

  $effect(() => { setLang(config.language ?? "zh"); });

  async function load(): Promise<void> {
    try {
      const [s, q, tr, c] = await Promise.all([
        api.getSummary("day"),
        api.getQuotas(),
        api.getTrends("day"),
        api.getConfig(),
      ]);
      summary = s;
      quotas = q;
      trends = tr;
      config = c;
    } catch { /* surfaced by the empty state */ }
    loadAttempted = true;
  }
  void load();

  // Live refresh: today's usage, quotas, and trend points each have their own
  // event; a short debounce coalesces collector bursts into one reload.
  $effect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const schedule = () => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => {
        timer = null;
        void load();
      }, 400);
    };
    const subs = [TODAY_UPDATED, QUOTA_UPDATED, COLLECTION_UPDATED].map((ev) =>
      listen<void>(ev, schedule),
    );
    return () => {
      if (timer) clearTimeout(timer);
      for (const p of subs) p.then((un) => un());
    };
  });

  // The two tightest quota windows across all vendors (used_pct desc).
  const tightQuotas = $derived.by(() => {
    const rows: { vendor: string; label: string; usedPct: number }[] = [];
    for (const q of quotas) {
      for (const w of q.windows ?? []) {
        rows.push({ vendor: q.vendor, label: w.label, usedPct: w.used_pct });
      }
    }
    return rows.sort((a, b) => b.usedPct - a.usedPct).slice(0, 2);
  });

  // Sparkline geometry over the 7 daily points.
  const spark = $derived.by(() => {
    const pts = trends?.points ?? [];
    if (pts.length < 2) return null;
    const W = 218, H = 30;
    const max = Math.max(1, ...pts.map((p) => p.tokens));
    const coords = pts.map((p, i) => {
      const x = (i / (pts.length - 1)) * W;
      const y = H - 3 - (p.tokens / max) * (H - 6);
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    });
    return { W, H, line: coords.join(" "), area: `0,${H} ${coords.join(" ")} ${W},${H}` };
  });

  const tokenText = $derived(summary ? splitTokensCN(summary.total_tokens, 2) : null);
  const costParts = $derived(summary ? splitCost(summary.cost_usd, config.currency ?? "both", 7.16) : null);
  const rateText = $derived(
    summary ? formatLiveRate("speed", summary.live_rate_speed, summary.live_rate_burn) : "",
  );

  async function closeWidget(): Promise<void> {
    // Persist the off state so the card stays hidden on next launch.
    try {
      await api.setConfig({ ...config, widget_enabled: false });
    } catch { /* config write failed — hide anyway */ }
    await getCurrentWindow().hide();
  }
</script>

<div class="widget" data-testid="widget-card">
  <div class="w-head" data-tauri-drag-region>
    <span class="w-day-label">{t("widget.today")}</span>
    <button type="button" class="w-close" title={t("widget.close")} aria-label={t("widget.close")} onclick={closeWidget}>✕</button>
  </div>

  {#if !summary && !loadAttempted}
    <div class="w-loading">…</div>
  {:else if !summary}
    <div class="w-loading">{t("common.loadFailed")}</div>
  {:else}
    <div class="w-hero">
      <span class="w-tokens">{tokenText?.value}<span class="w-unit">{tokenText?.unit}</span></span>
      <span class="w-cost">{#each costParts ?? [] as part, i (`${i}-${part.unit}`)}{part.sep ?? ""}<span class="w-cu">{part.unit}</span>{part.value}{/each}</span>
    </div>
    {#if rateText}
      <div class="w-rate">⚡ {rateText}</div>
    {/if}

    {#if tightQuotas.length > 0}
      <div class="w-quotas">
        {#each tightQuotas as q (q.vendor + q.label)}
          <div class="wq-row">
            <span class="wq-name">{VENDOR_LABELS[q.vendor] ?? q.vendor}<span class="wq-win">· {q.label}</span></span>
            <span class="wq-bar"><span class="wq-fill" class:f-warn={q.usedPct >= 80} class:f-ok={q.usedPct < 80} style="width:{Math.min(100, q.usedPct)}%"></span></span>
            <span class="wq-pct">{Math.round(q.usedPct)}%</span>
          </div>
        {/each}
      </div>
    {/if}

    {#if spark}
      <div class="w-spark">
        <svg viewBox="0 0 {spark.W} {spark.H}" preserveAspectRatio="none" aria-hidden="true">
          <defs>
            <linearGradient id="widget-spark-grad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stop-color="#e8b04b" stop-opacity="0.28" />
              <stop offset="100%" stop-color="#e8b04b" stop-opacity="0" />
            </linearGradient>
          </defs>
          <polygon points={spark.area} fill="url(#widget-spark-grad)" />
          <polyline points={spark.line} class="spark-line" />
        </svg>
        <span class="w-spark-label">{t("trends.last7days")}</span>
      </div>
    {/if}
  {/if}
</div>

<style>
  /* The window is transparent: the card itself carries the surface. */
  .widget {
    box-sizing: border-box;
    width: 100%;
    height: 100%;
    padding: 10px 14px 12px;
    background: var(--glass-2);
    border-radius: 14px;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.05), 0 18px 44px -14px rgba(0, 0, 0, 0.65);
    display: flex;
    flex-direction: column;
    gap: 7px;
    overflow: hidden;
    font-family: var(--font-ui);
    -webkit-user-select: none;
    user-select: none;
  }
  .w-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
  }
  .w-day-label {
    font-size: 0.6667rem;
    color: var(--text-faint);
    letter-spacing: 0.08em;
    cursor: default;
  }
  .w-close {
    background: none;
    border: none;
    color: var(--text-faint);
    font-size: 0.7333rem;
    cursor: pointer;
    padding: 2px 5px;
    border-radius: 4px;
    line-height: 1;
    -webkit-app-region: no-drag;
  }
  .w-close:hover { color: var(--coral); background: var(--coral-bg-soft); }

  .w-hero {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 8px;
  }
  /* Same amber ramp as the popover hero digits, at card scale. */
  .w-tokens {
    font-family: "Fraunces", var(--font-ui);
    font-size: 1.55rem;
    font-weight: 500;
    line-height: 1;
    background: linear-gradient(160deg, #f9e7bb, #e8b04b 52%, #cd943c);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .w-unit {
    font-size: 0.8rem;
    font-family: var(--font-ui);
    font-weight: 600;
    background: none;
    -webkit-text-fill-color: var(--text-dim);
  }
  .w-cost {
    font-size: 0.7667rem;
    font-weight: 600;
    color: var(--amber);
    white-space: nowrap;
    text-shadow: 0 0 14px rgba(232, 176, 75, 0.3);
  }
  .w-cost .w-cu { font-size: 0.6rem; margin-right: 1px; }
  .w-rate {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 0.6667rem;
    color: var(--text-faint);
    margin-top: -3px;
  }

  .w-quotas {
    display: flex;
    flex-direction: column;
    gap: 5px;
    padding-top: 6px;
    border-top: 1px dashed var(--border-dim);
  }
  .wq-row {
    display: flex;
    align-items: center;
    gap: 7px;
  }
  .wq-name {
    font-size: 0.6667rem;
    color: var(--text-dim);
    width: 92px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    flex-shrink: 0;
  }
  .wq-win { color: var(--text-faint); }
  .wq-bar {
    flex: 1;
    height: 5px;
    background: var(--bar-track);
    border-radius: 4px;
    overflow: hidden;
  }
  .wq-fill {
    display: block;
    height: 100%;
    border-radius: 4px;
    transition: width 0.3s;
  }
  .wq-fill.f-ok { background: linear-gradient(90deg, rgba(255, 255, 255, 0.45), rgba(242, 237, 225, 0.92)); }
  .wq-fill.f-warn { background: linear-gradient(90deg, rgba(232, 176, 75, 0.55), var(--amber)); }
  .wq-pct {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 0.6667rem;
    color: var(--text);
    font-weight: 600;
    width: 34px;
    text-align: right;
    flex-shrink: 0;
  }

  .w-spark {
    position: relative;
    margin-top: auto;
  }
  .w-spark svg { display: block; width: 100%; height: 30px; }
  .spark-line {
    fill: none;
    stroke: #e8b04b;
    stroke-width: 1.4;
    stroke-linejoin: round;
    stroke-linecap: round;
    vector-effect: non-scaling-stroke;
  }
  .w-spark-label {
    position: absolute;
    top: 0;
    right: 0;
    font-size: 0.5667rem;
    color: var(--text-faint);
  }
  .w-loading {
    font-size: 0.7333rem;
    color: var(--text-faint);
    padding: 18px 0;
    text-align: center;
  }

  :global([data-theme="light"]) .w-tokens {
    background: linear-gradient(160deg, #b57e1c, #9a6a12 55%, #7d5510);
    -webkit-background-clip: text;
    background-clip: text;
  }
  :global([data-theme="light"]) .wq-fill.f-ok {
    background: linear-gradient(90deg, rgba(62, 52, 32, 0.4), rgba(62, 52, 32, 0.78));
  }
</style>
