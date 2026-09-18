<script lang="ts">
  import { listen } from "@tauri-apps/api/event";
  import { invoke } from "@tauri-apps/api/core";
  import QuotaRing from "../components/pulse/QuotaRing.svelte";
  import DetailCard from "../components/pulse/DetailCard.svelte";

  interface PulseWindow {
    label: string;
    used_pct: number;
    resets_at?: string;
  }

  interface PulseBalance {
    amount: number;
    currency: string;
  }

  interface PulseQuota {
    vendor: string;
    plan?: string;
    status: string;
    windows: PulseWindow[];
    critical_pct: number;
    critical_label: string;
    balance?: PulseBalance;
  }

  interface PulseData {
    quotas: PulseQuota[];
    layout: string;
    size: string;
    ring_diameter: number;
    theme: string;
    position: string;
  }

  let data = $state<PulseData>({
    quotas: [],
    layout: "vertical",
    size: "medium",
    ring_diameter: 48,
    theme: "dark",
    position: "right",
  });

  let hoveredVendor = $state<string | null>(null);
  let isExpanded = $state(false);

  // Listen for data updates from Rust.
  listen<PulseData>("pulse:update", (e) => {
    data = e.payload;
  });

  listen<string | null>("pulse:expand", (e) => {
    isExpanded = true;
    hoveredVendor = e.payload;
  });

  listen("pulse:collapse", () => {
    isExpanded = false;
    hoveredVendor = null;
  });

  // Initial data fetch.
  async function init() {
    // Data is pushed via event on sync; no separate invoke needed.
  }
  init();

  function onRingHover(vendor: string) {
    hoveredVendor = vendor;
    invoke("expand_pulse", { vendor: vendor }).catch(() => {});
  }

  function onRingLeave() {
    hoveredVendor = null;
    invoke("collapse_pulse").catch(() => {});
  }

  function ringColor(pct: number): string {
    if (pct >= 80) return "var(--pulse-warning)";
    if (pct >= 50) return "var(--pulse-caution)";
    return "var(--pulse-good)";
  }

  let hoveredQuota = $derived(
    hoveredVendor
      ? data.quotas.find((q) => q.vendor === hoveredVendor) ?? null
      : null
  );
</script>

<div
  class="pulse-panel"
  class:dark={data.theme === "dark"}
  class:light={data.theme !== "dark"}
  class:horizontal={data.layout === "horizontal"}
  class:vertical={data.layout !== "horizontal"}
  class:expanded={isExpanded}
  style="--ring-size: {data.ring_diameter}px"
>
  <div class="ring-dock">
    {#each data.quotas as quota (quota.vendor)}
      <QuotaRing
        vendor={quota.vendor}
        pct={quota.critical_pct}
        label={quota.critical_label}
        diameter={data.ring_diameter}
        color={ringColor(quota.critical_pct)}
        onHover={() => onRingHover(quota.vendor)}
        onLeave={onRingLeave}
      />
    {/each}
    {#if data.quotas.length === 0}
      <div class="empty-ring" style="width:{data.ring_diameter}px;height:{data.ring_diameter}px">
        <span class="empty-icon">📊</span>
      </div>
    {/if}
  </div>

  {#if isExpanded && hoveredQuota}
    <DetailCard quota={hoveredQuota} position={data.position} />
  {/if}
</div>

<style>
  :root {
    --pulse-good: #00e68a;
    --pulse-caution: #ffc226;
    --pulse-warning: #ff4f42;
    --pulse-bg: rgba(0, 0, 0, 0.88);
    --pulse-card-bg: rgba(20, 18, 14, 0.95);
    --pulse-text: #f2ede3;
    --pulse-text-dim: #a39e94;
  }

  .light {
    --pulse-good: #00b36b;
    --pulse-caution: #cc8800;
    --pulse-warning: #cc2200;
    --pulse-bg: rgba(255, 255, 255, 0.92);
    --pulse-card-bg: rgba(245, 242, 236, 0.95);
    --pulse-text: #1a1610;
    --pulse-text-dim: #7a756c;
  }

  .pulse-panel {
    display: flex;
    background: var(--pulse-bg);
    border-radius: 16px;
    padding: 16px;
    user-select: none;
    -webkit-user-select: none;
    overflow: hidden;
    transition: all 0.25s cubic-bezier(0.4, 0, 0.2, 1);
  }

  .vertical {
    flex-direction: column;
    align-items: center;
    gap: 12px;
  }

  .horizontal {
    flex-direction: row;
    align-items: center;
    gap: 12px;
  }

  .ring-dock {
    display: flex;
    gap: 12px;
  }

  .vertical .ring-dock {
    flex-direction: column;
  }

  .horizontal .ring-dock {
    flex-direction: row;
  }

  .empty-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--pulse-text-dim);
    border-radius: 50%;
    opacity: 0.4;
  }

  .empty-icon {
    font-size: calc(var(--ring-size) * 0.4);
  }
</style>
