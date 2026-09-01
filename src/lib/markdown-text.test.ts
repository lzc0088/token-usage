import { describe, it, expect } from "vitest";
import { markdownToPlainText } from "./markdown-text";

describe("markdownToPlainText", () => {
  it("strips ATX heading markers but keeps the text", () => {
    expect(markdownToPlainText("## 更新内容")).toBe("更新内容");
    expect(markdownToPlainText("### Bug Fixes")).toBe("Bug Fixes");
    expect(markdownToPlainText("# v1.0.15")).toBe("v1.0.15");
  });

  it("unwraps bold / italic / code emphasis", () => {
    expect(markdownToPlainText("**重要**修复")).toBe("重要修复");
    expect(markdownToPlainText("__重要__修复")).toBe("重要修复");
    expect(markdownToPlainText("修复 *累计* 时段")).toBe("修复 累计 时段");
    expect(markdownToPlainText("修复 `each_key_duplicate` 异常")).toBe(
      "修复 each_key_duplicate 异常",
    );
  });

  it("keeps link text, drops the URL; removes images entirely", () => {
    expect(markdownToPlainText("见 [发布页](https://example.com) 详情")).toBe(
      "见 发布页 详情",
    );
    expect(markdownToPlainText("前 ![截图](https://x.com/a.png) 后")).toBe("前  后");
  });

  it("converts unordered list markers to bullets", () => {
    expect(markdownToPlainText("- 修复累计冻结\n- 新增热力图")).toBe(
      "• 修复累计冻结\n• 新增热力图",
    );
    expect(markdownToPlainText("* 第一项")).toBe("• 第一项");
  });

  it("keeps ordered list numbering as-is", () => {
    expect(markdownToPlainText("1. 第一步\n2. 第二步")).toBe("1. 第一步\n2. 第二步");
  });

  it("drops horizontal rules and fenced code blocks (keeps inner text)", () => {
    expect(markdownToPlainText("上文\n---\n下文")).toBe("上文\n下文");
    expect(markdownToPlainText("说明\n```rust\nfn main() {}\n```")).toBe(
      "说明\nfn main() {}",
    );
  });

  it("collapses 3+ blank lines to one and trims the ends", () => {
    expect(markdownToPlainText("\n\n## 更新\n\n\n- a\n\n\n")).toBe("更新\n• a");
  });

  it("passes plain text through unchanged", () => {
    expect(markdownToPlainText("普通文本，无样式")).toBe("普通文本，无样式");
    expect(markdownToPlainText("")).toBe("");
  });

  it("handles a realistic release body end-to-end", () => {
    const body = [
      "## v1.0.15 更新内容",
      "",
      "### 修复",
      "- **累计时段冻结**：Heatmap 月份标签重复 key（`each_key_duplicate`）",
      "- 趋势图切换时段不刷新",
      "",
      "详情见 [Release Notes](https://github.com/x/y/releases)",
    ].join("\n");
    expect(markdownToPlainText(body)).toBe(
      [
        "v1.0.15 更新内容",
        "修复",
        "• 累计时段冻结：Heatmap 月份标签重复 key（each_key_duplicate）",
        "• 趋势图切换时段不刷新",
        "详情见 Release Notes",
      ].join("\n"),
    );
  });
});
