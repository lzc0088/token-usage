<script lang="ts">
  // 额度 segment. Loads cached quotas from `get_quotas` once on open — no timer.
  // Refresh is driven by the background scheduler (quota_refresh_interval in settings).
  // Countdown timers for reset times still tick locally.
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import { t } from "../../lib/i18n.svelte";
  import { api, type Currency, type Quota } from "../../lib/api";
  import { QUOTA_UPDATED } from "../../lib/events";
  import QuotaCard from "../common/QuotaCard.svelte";
  import EmptyState from "../common/EmptyState.svelte";
  import Skeleton from "../common/Skeleton.svelte";

  let {
    currency = "cny" as Currency,
    cnyRate = 7.2,
    config,
  }: { currency?: Currency; cnyRate?: number; config?: import("../../lib/api").Config | null } = $props();

  let quotas = $state<Quota[] | null>(null);
  let nowMs = $state(Date.now());

  /** Open the settings window and navigate to a specific page. The target is
   *  passed to `open_settings` (Rust bridges it across the separate webview JS
   *  contexts); the settings window consumes it on focus and navigates. */
  async function openSettingsTo(partition: string): Promise<void> {
    try {
      await invoke("open_settings", { target: partition });
    } catch {
      /* settings open failed — user can still navigate manually */
    }
  }

  let visibleQuotas = $derived.by(() => {
    if (!quotas) return null;
    const activeVendors = config?.quota_active_vendors;
    // null/undefined = not configured → show all. Otherwise filter by the list
    // (empty array = user disabled all vendors → show none).
    if (activeVendors != null) {
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

  async function refresh(): Promise<void> {
    const gen = ++refreshGen;
    try {
      const q = await api.getQuotas();
      if (gen !== refreshGen) return;
      quotas = q;
    } catch (e) {
      /* quotas failed */
    }
  }

  $effect(() => {
    void refresh();
    const onFocus = () => { void refresh(); };
    window.addEventListener("focus", onFocus);
    // Reload when the popover becomes visible again (e.g. returning from the
    // settings window after clearing a credential). macOS suspends a hidden
    // webview, so the `quota:updated` event fired from the settings window can
    // be dropped — visibilitychange fires reliably on show and picks up the
    // fresh cache state.
    const onVis = () => { if (document.visibilityState === "visible") void refresh(); };
    document.addEventListener("visibilitychange", onVis);
    const un_quota = listen<void>(QUOTA_UPDATED, () => { void refresh(); });
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVis);
      un_quota.then((un) => un());
    };
  });

  $effect(() => {
    void api.refreshQuotasIfStale().catch(() => {});
  });
</script>

<div class="seg-body">
  {#if visibleQuotas === null}
    <Skeleton type="cards" />
  {:else if visibleQuotas.length === 0}
    <EmptyState
      title={quotas && quotas.length > 0 ? t("limits.allDisabled") : t("limits.noBinding")}
      hint={quotas && quotas.length > 0 ? t("limits.allDisabledHint") : t("limits.noBindingHint")}
      actionLabel={t("common.goSettings")}
      onAction={() => openSettingsTo("account")} />
  {:else}
    {@const progressMode = config?.quota_progress_mode ?? "剩余"}
    {#each visibleQuotas as q (q.vendor)}
      <QuotaCard quota={q} {progressMode} {nowMs} {currency} {cnyRate}  onQuotaChanged={refresh} />
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
</style>
