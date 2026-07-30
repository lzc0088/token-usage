<script lang="ts">
  // 窗口外观: 主题 / 动画 / 弹窗触发 / 窗口显示 / 菜单栏托盘.
  import type { Config } from "../../lib/api";
  let { config, onUpdate }: { config: Config; onUpdate: (p: Partial<Config>) => void } = $props();

  const THEME_OPTIONS: Array<{ value: NonNullable<Config["theme"]>; label: string }> = [
    { value: "dark", label: "深色" },
    { value: "light", label: "浅色" },
    { value: "system", label: "跟随系统" },
  ];

  const ANIMATION_OPTIONS: Array<{ value: NonNullable<Config["animation"]>; label: string }> = [
    { value: "on", label: "开启" },
    { value: "off", label: "关闭" },
    { value: "system", label: "跟随系统" },
  ];

  const activeTheme = $derived(config.theme || "system");
  const activeAnimation = $derived(config.animation || "system");

  // 菜单栏托盘显示方式的可选项（与 Rust default_tray_display 取值对齐）。
  const TRAY_OPTIONS: Array<{ value: NonNullable<Config["tray_display"]>; label: string }> = [
    { value: "today_tokens", label: "今日 Tokens" },
    { value: "today_cost", label: "今日成本" },
    { value: "today_both", label: "今日 Tokens + 成本" },
    { value: "total_tokens", label: "累计 Tokens" },
    { value: "total_cost", label: "累计成本" },
    { value: "total_both", label: "累计 Tokens + 成本" },
    { value: "icon_only", label: "仅显示图标" },
  ];

  // ── 快捷键录制 ──
  const MODIFIER_NAMES = new Set(["Alt", "Meta", "Shift", "Control"]);
  // 根据运行平台决定修饰键的显示名称
  const isMac = typeof navigator !== "undefined" && /Mac|iPod|iPhone|iPad/.test(navigator.platform ?? "");
  // accelerator 名 → 键盘按键显示名（全小写 key 做匹配）
  const KEY_LABELS: Record<string, string> = {
    alt:     isMac ? "OPT" : "ALT",    // Mac 物理键是 Option，Win/Linux 是 Alt
    option:  "OPT",
    meta:    isMac ? "CMD" : "WIN",    // Mac ⌘，Windows 徽标键
    command: "CMD",
    cmd:     "CMD",
    shift:   "SHIFT",
    control: "CTRL",
    ctrl:    "CTRL",
  };

  let recording = $state(false);

  /**
   * 将 accelerator 字符串（如 "Meta+Alt+T"）渲染为平台对应的键盘按键名。
   * 修饰键按平台映射，主键（字母/数字/功能键）保持大写。
   */
  function formatHotkey(hotkey: string): string[] {
    if (!hotkey) return [];
    return hotkey
      .split("+")
      .map((part) => part.trim())
      .filter(Boolean)
      .map((part) => {
        const label = KEY_LABELS[part.toLowerCase()];
        if (label) return label;
        if (part.length === 1) return part.toUpperCase();
        return part;
      });
  }

  function onSelect<K extends keyof Config>(key: K, value: Config[K]): void {
    onUpdate({ [key]: value } as Partial<Config>);
  }

  function startRecording(): void {
    recording = true;
  }

  function cancelRecording(): void {
    recording = false;
  }

  // 录制：keydown 组合 modifier + 主键。Escape 取消，Backspace（无主键时）清除。
  function onKeyDown(e: KeyboardEvent): void {
    if (!recording) return;
    e.preventDefault();
    e.stopPropagation();

    if (e.key === "Escape") {
      recording = false;
      return;
    }

    // 等待主键：纯修饰键按下时不结束录制。
    if (MODIFIER_NAMES.has(e.key)) return;

    const parts: string[] = [];
    if (e.metaKey) parts.push("Meta");
    if (e.ctrlKey) parts.push("Control");
    if (e.altKey) parts.push("Alt");
    if (e.shiftKey) parts.push("Shift");

    // 主键归一化：优先使用 e.code（物理按键，跨键盘布局一致），回退到 e.key。
    let main = e.code.replace(/^Key/, "").replace(/^Digit/, "").replace(/^Numpad/, "Numpad");
    if (!main || main.length > 2) {
      // e.code 异常时，用 e.key 兜底。
      main = e.key;
      if (!main || main.length > 2) return;
    }
    if (main === " ") {
      main = "Space";
    } else if (main.length === 1) {
      // 确保单个字符总是大写
      main = main.toUpperCase();
    }
    parts.push(main);

    onUpdate({ hotkey: parts.join("+") });
    recording = false;
  }
</script>

<svelte:window onkeydown={onKeyDown} />

<div class="sh"><h3>窗口外观</h3><div class="desc">主题、动画、弹窗触发与菜单栏显示方式</div></div>
<div class="sc">

  <!-- ══ 外观 ══ -->
  <div class="section-title">外观</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">主题<div class="hint">浅色 / 深色 / 跟随系统</div></div>
      <div class="seg">
        {#each THEME_OPTIONS as opt (opt.value)}
          <button
            type="button"
            class="seg-btn"
            class:on={activeTheme === opt.value}
            onclick={() => onUpdate({ theme: opt.value })}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>
    <div class="box-row">
      <div class="lab">动画<div class="hint">界面过渡与交互动画</div></div>
      <div class="seg">
        {#each ANIMATION_OPTIONS as opt (opt.value)}
          <button
            type="button"
            class="seg-btn"
            class:on={activeAnimation === opt.value}
            onclick={() => onUpdate({ animation: opt.value })}
          >
            {opt.label}
          </button>
        {/each}
      </div>
    </div>
  </div>

  <!-- ══ 行为 ══ -->
  <div class="section-title">行为</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">弹出方式<div class="hint">点击或移上菜单栏图标时显示窗口</div></div>
      <select
        class="sel"
        value={config.trigger_mode || "click"}
        onchange={(e) => {
          const target = e.target as HTMLSelectElement;
          onSelect("trigger_mode", target.value as Config["trigger_mode"]);
        }}
      >
        <option value="click">鼠标单击</option>
        <option value="hover">鼠标移上</option>
      </select>
    </div>

    <div class="box-row">
      <div class="lab">快捷方式<div class="hint">全局快捷键，随时显示 / 隐藏窗口</div></div>
      <div class="hotkey-wrap">
        {#if recording}
          <button type="button" class="hk recording" onclick={cancelRecording}>按下快捷键…</button>
        {:else if config.hotkey}
          <button type="button" class="hk" onclick={startRecording}>
            {#each formatHotkey(config.hotkey) as sym, i (i)}
              <kbd>{sym}</kbd>
            {/each}
          </button>
        {:else}
          <button type="button" class="hk empty" onclick={startRecording}>未设置</button>
        {/if}
      </div>
    </div>
  </div>

  <!-- ══ 显示 ══ -->
  <div class="section-title">显示</div>
  <div class="section-box">
    <div class="box-row">
      <div class="lab">窗口显示<div class="hint">仅针对主窗口 · 普通：可拖动记住位置 | 固定：贴托盘 | 置顶：浮在其他 App 上</div></div>
      <select
        class="sel"
        value={config.window_display_mode || "normal"}
        onchange={(e) => {
          const target = e.target as HTMLSelectElement;
          onSelect("window_display_mode", target.value as Config["window_display_mode"]);
        }}
      >
        <option value="normal">普通窗口</option>
        <option value="fixed">固定位置</option>
        <option value="always_on_top">浮在其他 App 上</option>
      </select>
    </div>

    <div class="box-row">
      <div class="lab">菜单托盘<div class="hint">菜单栏中图标的显示方式</div></div>
      <select
        class="sel"
        value={config.tray_display || "icon_only"}
        onchange={(e) => {
          const target = e.target as HTMLSelectElement;
          onSelect("tray_display", target.value as Config["tray_display"]);
        }}
      >
        {#each TRAY_OPTIONS as opt (opt.value)}
          <option value={opt.value}>{opt.label}</option>
        {/each}
      </select>
    </div>

    <div class="box-row">
      <div class="lab">程序坞图标<div class="hint">在 Dock 中显示应用图标（默认隐藏）</div></div>
      <div class="tg-placeholder">
        <button
          class="tg"
          class:on={!!config.show_in_dock}
          role="switch"
          aria-checked={!!config.show_in_dock}
          aria-label="程序坞图标"
          onclick={() => onUpdate({ show_in_dock: !config.show_in_dock })}
        ></button>
      </div>
    </div>
  </div>

</div>

<style>

  .sc { display: flex; flex-direction: column; }

  /* ── Make all right-side controls the same width ── */
  .sel { width: 150px; min-width: 150px; }
  .hk { width: 150px; min-width: 150px; }
  .tg-placeholder { width: 150px; display: flex; justify-content: flex-end; }

  /* ── segmented control ── */
  .seg {
    display: inline-flex;
    align-items: center;
    background: var(--glass-3);
    border: 1px solid var(--border-dim);
    border-radius: 8px;
    padding: 2px;
    gap: 1px;
    height: 32px;
    box-sizing: border-box;
  }
  .seg-btn {
    background: transparent;
    border: none;
    color: var(--text-faint);
    padding: 0 12px;
    height: 26px;
    border-radius: 6px;
    font-family: inherit;
    font-size: 12.5px;
    cursor: pointer;
    transition: all 0.15s;
    white-space: nowrap;
  }
  .seg-btn:hover { color: var(--text); }
  .seg-btn.on {
    background: var(--amber);
    color: var(--badge-text);
    font-weight: 500;
  }

  /* ── hotkey recorder ── */
  .hotkey-wrap { display: flex; align-items: center; gap: 6px; }
  .hk {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    background: var(--glass-subtle);
    border: 1px solid var(--border-dim);
    color: var(--amber);
    padding: 5px 10px;
    border-radius: 7px;
    font-family: inherit;
    font-size: 12px;
    cursor: pointer;
    height: 32px;
    box-sizing: border-box;
    transition: all 0.15s;
  }
  .hk:hover { border-color: var(--amber); }
  .hk.recording { color: var(--text-dim); border-color: var(--amber); border-style: dashed; animation: pulse 1.2s ease-in-out infinite; }
  .hk.empty { color: var(--text-faint); }
  @keyframes pulse {
    /* Avoid animating `opacity` — on macOS WKWebView with a transparent window,
       opacity changes trigger compositor rebuilds that can briefly hide
       sibling elements (e.g. the close button). Pulse the border instead. */
    0%, 100% { border-color: var(--amber); }
    50% { border-color: rgba(232, 176, 75, 0.3); }
  }
  .hk kbd {
    font-family: "JetBrains Mono", var(--font-mono);
    font-size: 12px;
    background: rgba(0,0,0,0.18);
    border: 1px solid var(--border-dim);
    border-radius: 4px;
    padding: 1px 6px;
    line-height: 1.4;
  }
</style>
