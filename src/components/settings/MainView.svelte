<script lang="ts">
  // 预览界面: 基本 + 布局 (tree with drag handle / expand / move / visibility).
  // All visibility/ordering is persisted to config (matching Collection/Account pattern).
  import { api, type Config } from "../../lib/api";
  import ToolIcon from "../../components/ui/ToolIcon.svelte";
  import { VENDOR_LABELS } from "../../lib/meta/vendors";
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

  const MODULE_LABELS: Record<string, string> = {
    overview: "总览", tools: "工具", models: "模型",
    projects: "项目", trends: "趋势", sessions: "会话", quotas: "额度",
  };
  const SUB_LABELS: Record<string, string> = {
    overview_io: "输入 / 输出 / 缓存",
    overview_tools: "工具",
    overview_models: "模型",
    overview_quotas: "额度",
  };

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
          const labels: Record<string, string> = {};
          for (const v of q) {
            labels[v.vendor] = VENDOR_LABELS[v.vendor] ?? v.vendor;
          }
          vendorLabels = labels;
        }
      } catch {}
    })();
    return () => { cancelled = true; };
  });

  // Rebuild when config or active vendors change.
  let navItems = $state<TreeItem[]>([]);
  let prevBuildSig = $state<string>("");
  $effect(() => {
    const sig = JSON.stringify([
      config?.layout_modules, config?.layout_overview_sub,
      config?.overview_quota_vendors, activeVendors,
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
    const vendorSet = new Set(quotaVendors ?? activeVendors);

    // Quota vendor grandchild items (dynamic, from active vendors).
    const quotaChildren: TreeItem[] = vendorOrder.map(v => ({
      key: `quota_vendor_${v}`,
      label: vendorLabels[v] ?? v,
      visible: vendorSet.has(v),
    }));

    const overviewChildren: TreeItem[] = subOrder.map(k => {
      if (k === "overview_quotas") {
        return {
          key: k,
          label: SUB_LABELS[k],
          visible: subSet.has(k),
          expanded: expanded.has(k),
          children: quotaChildren,
        };
      }
      return {
        key: k,
        label: SUB_LABELS[k],
        visible: subSet.has(k),
      };
    });

    return modOrder.map(key => {
      if (key === "overview") {
        return {
          key,
          label: MODULE_LABELS[key],
          visible: modSet.has(key),
          expanded: expanded.has(key),
          children: overviewChildren,
        };
      }
      return {
        key,
        label: MODULE_LABELS[key],
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

  /** Immutably rebuild the tree with `item` moved one slot in direction `dir`
   *  (-1 = up, +1 = down) within its own list. Creates fresh array references
   *  along the whole path so Svelte reactivity is unambiguous at every depth. */
  function withMovedItem(tree: TreeItem[], item: TreeItem, dir: -1 | 1): TreeItem[] {
    function rebuildList(list: TreeItem[]): TreeItem[] {
      const idx = list.indexOf(item);
      if (idx === -1) {
        // Not in this list — recurse immutably into children.
        return list.map(n => (n.children ? { ...n, children: rebuildList(n.children) } : n));
      }
      const j = idx + dir;
      if (j < 0 || j >= list.length) return list; // at edge, no move
      const next = [...list];
      const tmp = next[idx] as TreeItem;
      next[idx] = next[j] as TreeItem;
      next[j] = tmp;
      return next;
    }
    return rebuildList(tree);
  }

  function moveUp(item: TreeItem): void {
    const next = withMovedItem(navItems, item, -1);
    if (next === navItems) return;
    navItems = next;
    persistLayout();
  }

  function moveDown(item: TreeItem): void {
    const next = withMovedItem(navItems, item, 1);
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

<div class="sh"><h3>预览界面</h3><div class="desc">弹窗默认时段、数据刷新与模块布局</div></div>
<div class="sc">

  <!-- ══ 基本 ══ -->
  <div class="section-title">基本</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">默认时段<div class="hint">弹窗统计的默认时间范围</div></div>
      <select class="sel" value={config.default_period || "day"}
        onchange={(e) => onUpdate({ default_period: (e.target as HTMLSelectElement).value as Config["default_period"] })}>
        <option value="day">DAY（今日）</option>
        <option value="month">MONTH（本月）</option>
        <option value="total">TOTAL（全部）</option>
      </select>
    </div>
  </div>

  <!-- ══ 布局 ══ -->
  <div class="section-title">布局</div>
  <div class="section-box">

    <div class="icon-legend">
      <span class="legend-item">展开</span>
      <span class="legend-item">上移</span>
      <span class="legend-item">下移</span>
      <span class="legend-item">显示</span>
    </div>

    <div role="tree" aria-label="页面布局">

    {#each navItems as item, i (item.key)}
      <div class="tree-row" role="treeitem" aria-expanded={item.expanded ?? undefined} aria-level={1} aria-selected={false}>
        <span class="tree-left">
          <span class="grip" title="拖拽排序">
            <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>
          </span>
            {#if !["overview","tools","models","projects","trends","sessions","quotas"].includes(item.key)}
              <ToolIcon vendor={item.key} badge={false} size={14} />
            {/if}
            <span class="tree-label">{item.label}</span>
          </span>
        <span class="tree-right">
          {#if item.children}
            <button type="button" class="act" title={item.expanded ? '折叠' : '展开'} aria-expanded={item.expanded} aria-label={item.expanded ? `折叠 ${item.label}` : `展开 ${item.label}`} onclick={() => toggleExpand(item)}>
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                {#if item.expanded}
                  <line x1="5" y1="12" x2="19" y2="12"/>
                {:else}
                  <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                {/if}
              </svg>
            </button>
          {/if}
          <button type="button" class="mv" title="上移" disabled={i === 0} onclick={() => moveUp(item)}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>
            </svg>
          </button>
          <button type="button" class="mv" title="下移" disabled={i === navItems.length - 1} onclick={() => moveDown(item)}>
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
              <line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>
            </svg>
          </button>
          <button class="vis" class:on={item.visible} title={item.visible ? '显示中' : '已隐藏'} onclick={() => toggleVisible(item.key)}>
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
        {#each item.children as child, j (child.key)}
          <div class="tree-row child" role="treeitem" aria-expanded={child.expanded ?? undefined} aria-level={2} aria-selected={false}>
            <span class="tree-left">
              <span class="grip" title="拖拽排序">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>
              </span>
              <span class="tree-label">{child.label}</span>
            </span>
            <span class="tree-right">
              {#if child.children}
                <button type="button" class="act" title={child.expanded ? '折叠' : '展开'} aria-expanded={child.expanded} aria-label={child.expanded ? `折叠 ${child.label}` : `展开 ${child.label}`} onclick={() => toggleExpand(child)}>
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round">
                    {#if child.expanded}
                      <line x1="5" y1="12" x2="19" y2="12"/>
                    {:else}
                      <line x1="12" y1="5" x2="12" y2="19"/><line x1="5" y1="12" x2="19" y2="12"/>
                    {/if}
                  </svg>
                </button>
              {/if}
              <button type="button" class="mv" title="上移" disabled={j === 0} onclick={() => moveUp(child)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>
                </svg>
              </button>
              <button type="button" class="mv" title="下移" disabled={j === (item.children?.length ?? 1) - 1} onclick={() => moveDown(child)}>
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                  <line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>
                </svg>
              </button>
              <button class="vis" class:on={child.visible} title={child.visible ? '显示中' : '已隐藏'} disabled={!item.visible || isUnderHiddenParent(child)} onclick={() => toggleVisible(child.key)}>
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
            {#each child.children as grandchild, k (grandchild.key)}
              <div class="tree-row grandchild" role="treeitem" aria-level={3} aria-selected={false}>
                <span class="tree-left">
                  <span class="grip" title="拖拽排序">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="12" cy="5" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="12" cy="19" r="2"/></svg>
                  </span>
                  <span class="tree-label">{grandchild.label}</span>
                </span>
                <span class="tree-right">
                  <button type="button" class="mv" title="上移" disabled={k === 0} onclick={() => moveUp(grandchild)}>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                      <line x1="12" y1="19" x2="12" y2="5"/><polyline points="5 12 12 5 19 12"/>
                    </svg>
                  </button>
                  <button type="button" class="mv" title="下移" disabled={k === (child.children?.length ?? 1) - 1} onclick={() => moveDown(grandchild)}>
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round">
                      <line x1="12" y1="5" x2="12" y2="19"/><polyline points="19 12 12 19 5 12"/>
                    </svg>
                  </button>
                  <button class="vis" class:on={grandchild.visible} title={grandchild.visible ? '显示中' : '已隐藏'} disabled={!child.visible || isUnderHiddenParent(grandchild)} onclick={() => toggleVisible(grandchild.key)}>
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
    justify-content: flex-end;
    gap: 4px;
    margin-top: 0;
    margin-bottom: 0;
    padding: 0;
  }
  .icon-legend .legend-item {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    font-size: 10px;
    color: var(--text-faint);
    line-height: 1;
  }

  .tree-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 6px 0;
    border-bottom: none;
    gap: 12px;
  }
  .tree-row:last-child { border-bottom: none; }
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
  .act, .mv, .vis {
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
  .mv { color: var(--text-faint); }
  .vis { color: var(--text-faint); }

  .act:hover, .mv:hover:not(:disabled), .vis:hover {
    background: var(--glass-subtle-strong);
    color: var(--amber);
    transform: scale(1.05);
  }
  .act:active, .mv:active:not(:disabled), .vis:active {
    transform: scale(0.95);
  }
  .mv:disabled { opacity: 0.15; cursor: default; }
  .mv:disabled:hover { background: none; color: var(--text-faint); transform: none; }
  .vis.on { color: var(--amber); }
  .vis.on:hover { background: var(--amber-hover); }
</style>
