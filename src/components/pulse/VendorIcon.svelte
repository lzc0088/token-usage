<script lang="ts">
  // Presentational vendor glyph — renders the shared 24x24 mark from
  // src/lib/vendorIcons.ts at any size, as fill or stroke.
  import { vendorIcon } from "../../lib/vendorIcons";

  interface Props {
    vendor: string;
    size?: number;
    color?: string;
  }

  let { vendor, size = 16, color = "currentColor" }: Props = $props();

  let icon = $derived(vendorIcon(vendor));
</script>

{#if icon.d}
  <svg
    width={size}
    height={size}
    viewBox="0 0 24 24"
    aria-hidden="true"
    pointer-events="none"
  >
    <path
      d={icon.d}
      fill={icon.stroke ? "none" : color}
      stroke={icon.stroke ? color : "none"}
      stroke-width="1.8"
      stroke-linecap="round"
      stroke-linejoin="round"
    />
  </svg>
{/if}
