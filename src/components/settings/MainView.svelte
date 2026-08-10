<script lang="ts">
  // 预览界面: 基本 + 布局 (tree with drag handle / expand / move / visibility).
  // All visibility/ordering is persisted to config (matching Collection/Account pattern).
  import { api, type Config } from "../../lib/api";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import { rowDrag } from "../../lib/actions/rowDrag";
  import { t, getLang } from "../../lib/i18n.svelte";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();

  interface TreeItem {
    key: string;
    label: string;
    visible: boolean;
    expanded?: boolean;
    children?: TreeItem[];
  }

  const DEFAULT_MODULES = ["overview", "tools", "models", "projects", "trends", "sessions", "quotas"];
  const DEFAULT_OVERVIEW_SUB = ["overview_io", "overview_tools", "overview_models", "overview_quotas"];

  function modLabel(key: string): string {
    const m: Record<string,string> = { overview: t("seg.overview"), tools: t("seg.tools"), models: t("seg.models"), projects: t("seg.projects"), trends: t("seg.trends"), sessions: t("seg.sessions"), quotas: t("seg.limits") };
    return m[key] ?? key;
  }
  function subLabel(key: string): string {
    const m: Record<string,string> = { overview_io: t("mainview.overviewSub"), overview_tools: t("seg.tools"), overview_models: t("seg.models"), overview_quotas: t("seg.limits") };
    return m[key] ?? key;
  }

  /** Merge a stored (possibly partial) order with the canonical key list.
   *  Preserves stored order; appends any missing keys at the end (they are hidden
   *  in config, so their visibility flag will be false). */
  function mergeOrder<T>(known: readonly T[], stored: readonly T[] | null | undefined): T[] {
    const base = stored ?? known;
    const ordered = base.filter(k => known.includes(k));
    for (const k of known) {
      if (!ordered.includes(k)) ordered.push(k);
    }
    return ordered;
  }

  // Expanded state is tracked separately so it survives tree rebuilds
  // (moving items triggers a config-driven rebuild via $effect).
  const DEFAULT_EXPANDED = new Set<string>(["overview"]);
  let expandedKeys = $state<Set<string>>(new Set(DEFAULT_EXPANDED));

  // Load active vendors from quota data to build sub-items.
  let activeVendors = $state<string[]>([]);
  let vendorLabels = $state<Record<string, string>>({});

  $effect(() => {
    let cancelled = false;
    (async () => {
      try {
        const q = await api.getQuotas();
        if (!cancelled) {
          activeVendors = q.map(v => v.vendor);
          const l = getLang(); // current language for label map
          const labels: Record<string, string> = {};
          const zhMap: Record<string, string> = {
            deepseek: "DeepSeek ( 深度求索 )", glm: "GLM ( 智谱 )", minimax: "MiniMax ( 稀宇 )",
            kimi: "Kimi ( 月之暗面 )", volcengine: "Volcengine ( 火山方舟 )", mimo: "MiMo ( 小米 )",
            stepfun: "StepFun ( 阶跃星辰 )", iflytek: "iFlytek ( 讯飞星辰 )", copilot: "GitHub Copilot",
            zai_team: "GLM Team ( 智谱团队 )", claude: "Claude Code ( Anthropic )",
            codex: "Codex ( OpenAI )", opencode: "OpenCode ( OpenCode AI )",
            qoder: "Qoder ( 阿里 )", ollama: "Ollama ( Ollama Cloud )", cursor: "Cursor ( Anysphere )",
            grok: "Grok ( xAI )", openrouter: "OpenRouter",
          };
          const enMap: Record<string, string> = {
            deepseek: "DeepSeek", glm: "GLM", minimax: "MiniMax", kimi: "Kimi",
            volcengine: "Volcengine", mimo: "MiMo", stepfun: "StepFun", iflytek: "iFlytek",
            copilot: "GitHub Copilot", zai_team: "GLM Team", claude: "Claude Code",
            codex: "Codex", opencode: "OpenCode", qoder: "Qoder", ollama: "Ollama",
            cursor: "Cursor", grok: "Grok", openrouter: "OpenRouter",
          };
          const src = l === "en" ? enMap : zhMap;
          for (const v of q) {
            labels[v.vendor] = src[v.vendor] ?? v.vendor;
          }
          vendorLabels = labels;
        }
      } catch {}
    })();
    return () => { cancelled = true; };
  });

  // Rebuild when config, active vendors, OR language changes.
  let navItems = $state<TreeItem[]>([]);
  let prevBuildSig = $state<string>("");
  $effect(() => {
    const sig = JSON.stringify([
      config?.layout_modules, config?.layout_overview_sub,
      config?.overview_quota_vendors, activeVendors, getLang(),
      config?.quota_active_vendors,
      [...expandedKeys],
    ]);
    if (sig === prevBuildSig) return;
    prevBuildSig = sig;
    navItems = buildItems(
      config?.layout_modules, config?.layout_overview_sub,
      config?.overview_quota_vendors,
      expandedKeys,
    );
  });

  function buildItems(
    modKeys?: string[] | null,
    subKeys?: string[] | null,
    quotaVendors?: string[] | null,
    expanded: Set<string> = DEFAULT_EXPANDED,
  ): TreeItem[] {
    // Orders come from config (user reordering); missing keys appended at end.
    const modOrder = mergeOrder(DEFAULT_MODULES, modKeys);
    const subOrder = mergeOrder(DEFAULT_OVERVIEW_SUB, subKeys);
    const vendorOrder = mergeOrder(activeVendors, quotaVendors);

    const modSet = new Set(modKeys ?? DEFAULT_MODULES);
    const subSet = new Set(subKeys ?? DEFAULT_OVERVIEW_SUB);

    // Filter out vendors with no actual quota data (empty cache entries
    // are now filtered by the Rust backend, but double-check here too).
    // Only show vendors that are both enabled in Account (quota_active_vendors)
    // AND have cached data in the quota list. If quota_active_vendors is null
    // (never configured in Account), fall back to showing all with data.
    const accountEnabled = config?.quota_active_vendors;
    const accountSet = accountEnabled != null ? new Set(accountEnabled) : null;
    // Visibility comes from the persisted preview config (quotaVendors);
    // null means "never configured → show all". Non-null means only listed
    // vendors are visible — their toggle state drives persistLayout(), which
    // writes back to overview_quota_vendors.
    const previewSet = quotaVendors != null ? new Set(quotaVendors) : null;
    const quotaChildren: TreeItem[] = vendorOrder
      .filter(v => vendorLabels[v]) // has data
      .filter(v => accountSet == null || accountSet.has(v)) // enabled in Account
      .map(v => ({
        key: `quota_vendor_${v}`,
        label: vendorLabels[v]!,
        visible: previewSet != null ? previewSet.has(v) : true,
      }));

    const overviewChildren: TreeItem[] = subOrder.map(k => {
      if (k === "overview_quotas") {
        return {
          key: k,
          label: subLabel(k),
          visible: subSet.has(k),
          expanded: expanded.has(k),
          children: quotaChildren,
        };
      }
      return {
        key: k,
        label: subLabel(k),
        visible: subSet.has(k),
      };
    });

    return modOrder.map(key => {
      if (key === "overview") {
        return {
          key,
          label: modLabel(key),
          visible: modSet.has(key),
          expanded: expanded.has(key),
          children: overviewChildren,
        };
      }
      return {
        key,
        label: modLabel(key),
        visible: modSet.has(key),
      };
    });
  }

  function persistLayout(): void {
    // Build the full tree first to know which items are under hidden parents
    const modules = navItems.filter(m => m.visible).map(m => m.key);

    // Overview subs: only include if overview itself is visible, and exclude subs under hidden parents
    const overview = navItems.find(m => m.key === "overview");
    const sub: string[] = [];
    if (overview?.visible && overview.children) {
      for (const child of overview.children) {
        // Only include subs that are visible AND not a quota_vendor_ item
        if (child.visible && !child.key.startsWith("quota_vendor_")) {
          sub.push(child.key);
        }
      }
    }

    // Extract quota vendor visibility: only if overview_quotas is visible
    const quotasChild = overview?.children?.find(c => c.key === "overview_quotas");
    const quotaVendors: string[] = [];
    if (quotasChild?.visible && quotasChild.children) {
      for (const v of quotasChild.children) {
        if (v.visible) {
          quotaVendors.push(v.key.replace("quota_vendor_", ""));
        }
      }
    }

    onUpdate({ layout_modules: modules, layout_overview_sub: sub, overview_quota_vendors: quotaVendors });
  }

  function toggleVisible(targetKey: string): void {
    function update(items: TreeItem[]): TreeItem[] {
      return items.map(n => {
        if (n.key === targetKey) return { ...n, visible: !n.visible };
        if (n.children) return { ...n, children: update(n.children) };
        return n;
      });
    }
    navItems = update(navItems);
    persistLayout();
  }
  function toggleExpand(p: TreeItem): void {
    const next = new Set(expandedKeys);
    if (next.has(p.key)) {
      next.delete(p.key);
    } else {
      next.add(p.key);
    }
    expandedKeys = next;
  }

  /** Move a tree item (by key) to `newIndex` within its parent's children
   *  array. Immutable — returns a fresh tree, or the original if the item
   *  isn't found or the index would be unchanged. */
  function moveTreeItem(tree: TreeItem[], itemKey: string, newIndex: number): TreeItem[] {
    function walk(items: TreeItem[]): TreeItem[] | null {
      for (let i = 0; i < items.length; i++) {
        if (items[i].key === itemKey) {
          if (i === newIndex) return null; // no change
          const out = [...items];
          const [item] = out.splice(i, 1);
          const insertAt = newIndex > i ? newIndex - 1 : newIndex;
          out.splice(insertAt, 0, item!);
          return out;
        }
        if (items[i].children) {
          const result = walk(items[i].children!);
          if (result != null) {
            const out = [...items];
            out[i] = { ...out[i]!, children: result };
            return out;
          }
        }
      }
      return null;
    }
    return walk(tree) ?? tree;
  }

  /** Called by `use:rowDrag` on drop. */
  function moveToIndex(key: string, newIndex: number): void {
    const next = moveTreeItem(navItems, key, newIndex);
    if (next === navItems) return;
    navItems = next;
    persistLayout();
  }

  /** True if any ancestor of `target` is hidden (visibility toggle should be disabled). */
  function isUnderHiddenParent(target: TreeItem): boolean {
    function walk(items: TreeItem[], hiddenAncestor: boolean): boolean | null {
      for (const item of items) {
        if (item === target) return hiddenAncestor;
        if (item.children) {
          const r = walk(item.children, hiddenAncestor || !item.visible);
          if (r !== null) return r;
        }
      }
      return null;
    }
    return walk(navItems, false) ?? false;
  }
</script>

<div class="sh"><h3>{t("mainview.title")}</h3><div class="desc">{t("mainview.desc")}</div></div>
<div class="sc">

  <!-- ══ 基本 ══ -->
  <div class="section-title">{t("mainview.basic")}</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">{t("mainview.defaultPeriod")}<div class="hint">{t("mainview.defaultPeriodHint")}</div></div>
      <select class="sel" value={config.default_period || "day"}
        onchange={(e) => onUpdate({ default_period: (e.target as HTMLSelectElement).value as Config["default_period"] })}>
        <option value="day">{t("mainview.periodDay")}</option>
        <option value="month">{t("mainview.periodMonth")}</option>
        <option value="total">{t("mainview.periodTotal")}</option>
      </select>
    </div>
  </div>

  <!-- ══ 布局 ══ -->
  <div class="section-title">{t("mainview.layout")}</div>
  <div class="section-box">

    <div class="icon-legend">
      <span class="legend-text">{t("account.dragReorder")}</span>
      <div class="legend-actions">
        <span class="legend-item">{t("mainview.expand")}</span>
        <span class="legend-item">{t("mainview.show")}</span>
      </div>
    </div>

    <div role="tree" aria-label="页面布局">

    {#each navItems as item (item.key)}
      <div
        class="tree-row"
        role="treeitem"
        aria-expanded={item.expanded ?? undefined}
        aria-level={1}
        aria-selected={false}
        data-row-id={item.key}
        use:rowDrag={{ id: item.key, onReorder: (newIndex) => moveToIndex(item.key, newIndex), siblingSelector: '[aria-level="1"]', excludeSelector: "button" }}
      >
        <span class="tree-left">
          <span class="grip" title={t("mainview.dragHint")}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>
          </span>
            {#if !["overview","tools","models","projects","trends","sessions","quotas"].includes(item.key)}
              <ToolIcon vendor={item.key} badge={false} size={14} />
            {/if}
            <span class="tree-label">{item.label}</span>
          </span>
        <span class="tree-right">
          {#if item.children}
            <button type="button" class="act" title={item.expanded ? t("mainview.collapse") : t("mainview.expand")} aria-expanded={item.expanded} aria-label={item.expanded ? `折叠 ${item.label}` : `展开 ${item.label}`} onclick={() => toggleExpand(item)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                {#if item.expanded}
                  <line x1="5" y1="12" x2="19" y2="12"/>
                {:else}
                  <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                {/if}
              </svg>
            </button>
          {/if}
          <button class="vis" class:on={item.visible} title={item.visible ? t("mainview.visible") : t("mainview.hidden")} onclick={() => toggleVisible(item.key)}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              {#if item.visible}
                <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>
              {:else}
                <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/>
              {/if}
            </svg>
          </button>
        </span>
      </div>

      {#if item.children && item.expanded}
        {#each item.children as child (child.key)}
          <div
            class="tree-row child"
            role="treeitem"
            aria-expanded={child.expanded ?? undefined}
            aria-level={2}
            aria-selected={false}
            data-row-id={child.key}
            use:rowDrag={{ id: child.key, onReorder: (newIndex) => moveToIndex(child.key, newIndex), siblingSelector: '[aria-level="2"]', excludeSelector: "button" }}
          >
            <span class="tree-left">
              <span class="grip" title={t("mainview.dragHint")}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>
              </span>
              <span class="tree-label">{child.label}</span>
            </span>
            <span class="tree-right">
              {#if child.children}
                <button type="button" class="act" title={child.expanded ? t("mainview.collapse") : t("mainview.expand")} aria-expanded={child.expanded} aria-label={child.expanded ? `折叠 ${child.label}` : `展开 ${child.label}`} onclick={() => toggleExpand(child)}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    {#if child.expanded}
                      <line x1="5" y1="12" x2="19" y2="12"/>
                    {:else}
                      <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                    {/if}
                  </svg>
                </button>
              {/if}
              <button class="vis" class:on={child.visible} title={child.visible ? t("mainview.visible") : t("mainview.hidden")} disabled={!item.visible || isUnderHiddenParent(child)} onclick={() => toggleVisible(child.key)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  {#if child.visible}
                    <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>
                  {:else}
                    <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/>
                  {/if}
                </svg>
              </button>
            </span>
          </div>
          {#if child.children && child.expanded}
            {#each child.children as grandchild (grandchild.key)}
              <div
                class="tree-row grandchild"
                role="treeitem"
                aria-level={3}
                aria-selected={false}
                data-row-id={grandchild.key}
                use:rowDrag={{ id: grandchild.key, onReorder: (newIndex) => moveToIndex(grandchild.key, newIndex), siblingSelector: '[aria-level="3"]', excludeSelector: "button" }}
              >
                <span class="tree-left">
                  <span class="grip" title={t("mainview.dragHint")}>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>
                  </span>
                  <span class="tree-label">{grandchild.label}</span>
                </span>
                <span class="tree-right">
                  <button class="vis" class:on={grandchild.visible} title={grandchild.visible ? t("mainview.visible") : t("mainview.hidden")} disabled={!child.visible || isUnderHiddenParent(grandchild)} onclick={() => toggleVisible(grandchild.key)}>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                      {#if grandchild.visible}
                        <path d="M1 12s4-8 11-8 11 8 11 8-4 8-11 8-11-8-11-8z"/><circle cx="12" cy="12" r="3"/>
                      {:else}
                        <path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/>
                      {/if}
                    </svg>
                  </button>
                </span>
              </div>
            {/each}
          {/if}
        {/each}
      {/if}
    {/each}
    </div><!-- role=tree -->
  </div>

</div>

<style>

  .sc { display: flex; flex-direction: column; }

  /* ── tree ── */
  .icon-legend {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-top: 0;
    margin-bottom: 0;
    padding: 0;
    gap: 4px;
  }
  .icon-legend .legend-actions {
    display: flex;
    align-items: center;
    gap: 2px; /* match .tree-right button gap */
  }
  .icon-legend .legend-item {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px; /* match .act/.vis button width */
    height: 28px;
    font-size: 10.5px;
    color: var(--text-faint);
    line-height: 1;
  }
  .icon-legend .legend-text {
    font-size: 10.5px;
    color: var(--text-faint);
    white-space: nowrap;
  }

  .tree-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 4px 6px 6px;
    border-bottom: none;
    gap: 12px;
    border-radius: 4px;
    cursor: grab;
    transition: background 0.12s;
  }
  .tree-row:hover { background: var(--surface-tint); }
  .tree-row:active { cursor: grabbing; }
  .tree-row:last-child { border-bottom: none; }
  .tree-row:global(.row-drag-source) { opacity: 0.35; }
  :global(.row-drag-ghost) {
    pointer-events: none;
    opacity: 0.75;
    box-shadow: 0 6px 18px rgba(0, 0, 0, 0.45);
    background: var(--surface-tint-strong);
    border-radius: 5px;
    will-change: transform;
  }
  .tree-row.child .tree-label { padding-left: 18px; }
  .tree-row.grandchild .tree-label { padding-left: 36px; font-size: 11px; color: var(--text-dim); }

  .tree-left {
    display: flex;
    align-items: center;
    gap: 4px;
    min-width: 0;
    flex: 1;
  }

  .tree-label {
    font-size: 12.5px;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .grip {
    display: inline-flex;
    align-items: center;
    color: var(--text-faint);
    cursor: grab;
    flex-shrink: 0;
    padding: 1px 0;
  }
  .grip:hover { color: var(--amber); }

  .tree-right {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-shrink: 0;
  }
  .tree-right svg { width: 14px; height: 14px; }
  .tree-left svg { width: 14px; height: 14px; }

  /* ── icon buttons (shared) ── */
  .act, .vis {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 24px;
    background: none;
    border: 1px solid transparent;
    border-radius: 5px;
    cursor: pointer;
    padding: 0;
    flex-shrink: 0;
    transition: all 0.15s;
  }
  .act { color: var(--amber); }
  .vis { color: var(--text-faint); }

  .act:hover, .vis:hover {
    background: var(--glass-subtle-strong);
    color: var(--amber);
    transform: scale(1.05);
  }
  .act:active, .vis:active {
    transform: scale(0.95);
  }
  .vis.on { color: var(--amber); }
  .vis.on:hover { background: var(--amber-hover); }
</style>
