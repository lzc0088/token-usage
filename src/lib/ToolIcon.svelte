<script lang="ts">
  // Reusable tool/vendor icon component.
  //
  // Modes:
  //   badge  (default) = 28px colored rounded square — for breakdown rows, overview cards
  //   inline           = 14px standalone icon — for session rows, detail labels, quota headers
  //
  // Props:
  //   tool   — tool key (e.g. "claude", "cursor") → looks up icon + color from toolMeta
  //   vendor — vendor key (e.g. "deepseek", "stepfun") → looks up icon + color
  //   badge  — if true, render as 28px colored badge; else 14px inline icon
  //   size   — custom icon size in px (only for inline mode)

  import { toolMeta, vendorIcon } from "./toolMeta";

  let {
    tool,
    vendor,
    badge = false,
    size = 14,
  }: {
    tool?: string;
    vendor?: string;
    badge?: boolean;
    size?: number;
  } = $props();

  // Resolve meta: try tool first, then vendor.
  let meta = $derived.by(() => {
    if (tool) {
      const tm = toolMeta(tool);
      if (tm.icon) return tm;
    }
    if (vendor) {
      const icon = vendorIcon(vendor);
      if (icon) {
        return {
          label: toolMeta(vendor).label,
          icon,
          color: "var(--text-dim)",
        };
      }
    }
    return toolMeta(tool ?? vendor ?? "?");
  });

  let svg = $derived(meta.icon);
  let iconSize = $derived(badge ? 28 : size);
</script>

{#if badge}
  <span
    class="ti-badge"
    style="width:{iconSize}px;height:{iconSize}px;background:{meta.color};color:#1a1408"
    title={meta.label}
  >
    {@html svg}
  </span>
{:else}
  <span
    class="ti-inline"
    style="width:{iconSize}px;height:{iconSize}px;color:{meta.color}"
    title={meta.label}
  >
    {@html svg}
  </span>
{/if}

<style>
  .ti-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 7px;
    flex-shrink: 0;
  }
  .ti-badge :global(svg) {
    width: 18px;
    height: 18px;
  }
  .ti-inline {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
  }
  .ti-inline :global(svg) {
    width: 100%;
    height: 100%;
  }
</style>
