<script lang="ts">
  // 额度 segment. Loads cached quotas from `get_quotas` once on open — no timer.
  // Refresh is driven by the background scheduler (quota_refresh_interval in settings).
  // Countdown timers for reset times still tick locally.
  import { listen } from "@tauri-apps/api/event";
  import { api, type Config, type Currency, type Quota } from "../../lib/api";
  import QuotaCard from "../common/QuotaCard.svelte";

  let { currency = "cny" as Currency, cnyRate = 7.2 }: { currency?: Currency; cnyRate?: number } = $props();

  let quotas = $state<Quota[] | null>(null);
  let config = $state<Config | null>(null);
  let nowMs = $state(Date.now());

  let visibleQuotas = $derived.by(() => {
    if (!quotas) return null;
    const activeVendors = config?.quota_active_vendors;
    if (activeVendors && activeVendors.length > 0) {
      const set = new Set(activeVendors);
      return quotas.filter(q => set.has(q.vendor));
    }
    return quotas;
  });

  // Tick every 30s so reset countdowns stay live (local only, no API calls).
  $effect(() => {
    const t = setInterval(() => { nowMs = Date.now(); }, 30_000);
    return () => clearInterval(t);
  });

  // Generation counter: prevents a slow IPC response from overwriting fresher
  // data that arrived via a later `quota:updated` event.
  let refreshGen = 0;

  // Load on mount + re-fetch on window focus (e.g. settings changed).
  async function refresh(): Promise<void> {
    const gen = ++refreshGen;
    try {
      const [q, c] = await Promise.all([api.getQuotas(), api.getConfig()]);
      if (gen !== refreshGen) return;
      quotas = q;
      config = c;
    } catch (e) {
      /* quotas failed */
    }
  }

  $effect(() => {
    void refresh();
    const onFocus = () => { void refresh(); };
    window.addEventListener("focus", onFocus);
    const un_quota = listen<void>("quota:updated", () => { void refresh(); });
    // config:changed covers enable/hide + progressMode toggles from settings.
    const un_config = listen<void>("config:changed", () => { void refresh(); });
    return () => {
      window.removeEventListener("focus", onFocus);
      un_quota.then((un) => un());
      un_config.then((un) => un());
    };
  });

  $effect(() => {
    void api.refreshQuotasIfStale().catch(() => {});
  });
</script>

<div class="seg-body">
  {#if visibleQuotas === null}
    <p class="loading">加载中…</p>
  {:else if visibleQuotas.length === 0}
    <div class="empty">
      {#if quotas && quotas.length > 0}
        <p>所有厂商已停用</p>
        <p class="hint">在「设置 → 账号」中启用的厂商额度将显示在此</p>
      {:else}
        <p>未绑定厂商账号</p>
        <p class="hint">在「设置 → 账号」绑定 API Key / OAuth 后，额度将显示在此</p>
      {/if}
    </div>
  {:else}
    {@const progressMode = config?.quota_progress_mode ?? "剩余"}
    {#each visibleQuotas as q (q.vendor)}
      <QuotaCard quota={q} {progressMode} {nowMs} {currency} {cnyRate} />
    {/each}
  {/if}
</div>

<style>
  .seg-body {
    padding: 14px 16px 18px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .loading {
    color: var(--text-faint);
    font-size: 12px;
    padding: 24px 0;
    text-align: center;
  }
  .empty {
    text-align: center;
    padding: 32px 16px;
  }
  .empty p {
    margin: 0;
    color: var(--text-dim);
    font-size: 13px;
  }
  .empty .hint {
    margin-top: 6px;
    font-size: 11px;
    color: var(--text-faint);
  }
</style>
