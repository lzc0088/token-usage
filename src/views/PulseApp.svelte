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
    second_pct?: number;
    second_label?: string;
    is_running?: boolean;
    is_refreshing?: boolean;
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

  async function dismissPulse() {
    await invoke("dismiss_pulse").catch(() => {});
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
  class:pos-right={data.position === "right"}
  class:pos-left={data.position === "left"}
  class:pos-top={data.position === "top"}
  style="--ring-size: {data.ring_diameter}px"
>
  <!-- Close / dismiss button -->
  <button
    class="close-btn"
    onclick={dismissPulse}
    aria-label="关闭 Pulse"
    title="关闭 Pulse"
  >
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
      <path
        d="M1 1L9 9M9 1L1 9"
        stroke="currentColor"
        stroke-width="1.5"
        stroke-linecap="round"
      />
    </svg>
  </button>

  <!-- Ring rail -->
  <div class="ring-dock">
    {#each data.quotas as quota (quota.vendor)}
      <QuotaRing
        vendor={quota.vendor}
        pct={quota.critical_pct}
        label={quota.critical_label}
        diameter={data.ring_diameter}
        onHover={() => onRingHover(quota.vendor)}
        onLeave={onRingLeave}
        isRunning={quota.is_running ?? false}
        isRefreshing={quota.is_refreshing ?? false}
        secondPct={quota.second_pct}
        showsRemaining={false}
      />
    {/each}
    {#if data.quotas.length === 0}
      <div
        class="empty-ring"
        style="width:{data.ring_diameter}px;height:{data.ring_diameter}px"
      >
        <span class="empty-icon">📊</span>
      </div>
    {/if}
  </div>

  <!-- Detail card (flies out on hover) -->
  {#if isExpanded && hoveredQuota}
    <DetailCard
      quota={hoveredQuota}
      position={data.position}
    />
  {/if}
</div>

<style>
  :root {
    --pulse-good: #00e68a;
    --pulse-caution: #ffc226;
    --pulse-warning: #ff4f42;
    --pulse-exhausted: #d92027;
    --pulse-bg: #000000;
    --pulse-ring-bg: #000000;
    --pulse-card-bg: rgba(12, 12, 12, 0.96);
    --pulse-text: #f2ede3;
    --pulse-text-dim: #8a857b;
    --pulse-track: rgba(255, 255, 255, 0.12);
    --pulse-bar-track: rgba(255, 255, 255, 0.10);
  }

  .light {
    --pulse-good: #00b36b;
    --pulse-caution: #cc8800;
    --pulse-warning: #cc2200;
    --pulse-exhausted: #a81820;
    --pulse-bg: rgba(240, 238, 234, 0.96);
    --pulse-ring-bg: rgba(240, 238, 234, 0.96);
    --pulse-card-bg: rgba(250, 248, 244, 0.96);
    --pulse-text: #1a1610;
    --pulse-text-dim: #7a756c;
    --pulse-track: rgba(0, 0, 0, 0.10);
    --pulse-bar-track: rgba(0, 0, 0, 0.07);
  }

  /* ── Panel surface (berth fusion) ──────────────────────────────────── */

  .pulse-panel {
    position: relative;
    display: flex;
    background: var(--pulse-bg);
    color: var(--pulse-text);
    user-select: none;
    -webkit-user-select: none;
    overflow: visible;
    transition: border-radius 0.32s cubic-bezier(0.34, 1.56, 0.64, 1);
    /* Berth shape: fully rounded on the outer edge, flat on the screen-fused
       edge. Pulse fuses to the screen edge, so the edge touching the border
       is flat — no gap, no rounded corner showing wallpaper. */
    border-radius: 20px;
    backdrop-filter: blur(24px) saturate(180%);
    -webkit-backdrop-filter: blur(24px) saturate(180%);
  }

  /* Position-dependent berth: right edge is flat (fused), left is rounded. */
  .pulse-panel.pos-right {
    border-top-left-radius: 20px;
    border-bottom-left-radius: 20px;
    border-top-right-radius: 0;
    border-bottom-right-radius: 0;
    margin-right: -1px; /* fuse to screen edge */
  }

  .pulse-panel.pos-left {
    border-top-right-radius: 20px;
    border-bottom-right-radius: 20px;
    border-top-left-radius: 0;
    border-bottom-left-radius: 0;
    margin-left: -1px;
  }

  .pulse-panel.pos-top {
    border-bottom-left-radius: 20px;
    border-bottom-right-radius: 20px;
    border-top-left-radius: 0;
    border-top-right-radius: 0;
    margin-top: -1px;
  }

  /* ── Layout ────────────────────────────────────────────────────────── */

  .vertical {
    flex-direction: column;
    align-items: center;
    padding: 14px 16px;
    gap: 8px;
  }

  .horizontal {
    flex-direction: row;
    align-items: center;
    padding: 16px 14px;
    gap: 8px;
  }

  /* ── Close button ──────────────────────────────────────────────────── */

  .close-btn {
    position: absolute;
    top: 6px;
    right: 6px;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(128, 128, 128, 0.2);
    border: none;
    border-radius: 50%;
    color: var(--pulse-text-dim);
    cursor: pointer;
    padding: 0;
    transition: all 0.15s ease;
    opacity: 0;
    pointer-events: none;
    z-index: 10;
  }

  .pulse-panel:hover .close-btn {
    opacity: 1;
    pointer-events: auto;
  }

  .close-btn:hover {
    background: rgba(255, 79, 66, 0.3);
    color: var(--pulse-warning);
    transform: scale(1.1);
  }

  .close-btn:active {
    transform: scale(0.92);
  }

  /* ── Ring dock ─────────────────────────────────────────────────────── */

  .ring-dock {
    display: flex;
    gap: 10px;
  }

  .vertical .ring-dock {
    flex-direction: column;
  }

  .horizontal .ring-dock {
    flex-direction: row;
  }

  /* ── Empty state ───────────────────────────────────────────────────── */

  .empty-ring {
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--pulse-text-dim);
    border-radius: 50%;
    opacity: 0.3;
  }

  .empty-icon {
    font-size: calc(var(--ring-size) * 0.4);
  }
</style>
