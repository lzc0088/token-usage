/**
 * Model → vendor (厂商) mapping.
 *
 * Infers the vendor from a model name's prefix (e.g. "glm-5.2" → "智谱").
 * Used to show the vendor label next to the model name in the Overview.
 */

export interface VendorMeta {
  vendor: string;
  /** Display color for the vendor tag. */
  color: string;
}

/**
 * Rules ordered by specificity (longer/more-specific prefixes first).
 * Each rule: [prefix(es), vendor, color].
 */
const RULES: [string[], string, string][] = [
  [["claude", "anthropic"], "Anthropic", "var(--amber)"],
  [["gpt", "o1", "o3", "o4", "chatgpt", "text-embedding", "text-davinci", "davinci"], "OpenAI", "var(--violet)"],
  [["gemini", "gemma"], "Google", "var(--cyan)"],
  [["deepseek"], "深度求索", "var(--cyan)"],
  [["glm", "zai", "zhipu", "chatglm"], "智谱", "var(--amber)"],
  [["qwen", "qwq", "tongyi"], "阿里", "var(--violet)"],
  [["step"], "阶跃星辰", "var(--lime)"],
  [["kimi", "moonshot"], "月之暗面", "var(--cyan)"],
  [["llama"], "Meta", "var(--text-dim)"],
  [["mistral", "mixtral", "codestral", "magistral"], "Mistral", "var(--amber)"],
  [["yi"], "零一万物", "var(--lime)"],
  [["baichuan"], "百川", "var(--coral)"],
  [["spark"], "讯飞星火", "var(--cyan)"],
  [["hunyuan", "hy"], "腾讯混元", "var(--amber)"],
  [["ernie", "wenxin"], "百度文心", "var(--violet)"],
  [["doubao", "skylark"], "字节豆包", "var(--coral)"],
  [["mimo"], "小米 MiMo", "var(--cyan)"],
  [["auto", "ark"], "火山方舟", "var(--coral)"],
  [["minimax", "abab"], "MiniMax", "var(--lime)"],
  [["grok"], "xAI", "var(--coral)"],
  [["phi"], "Microsoft", "var(--cyan)"],
  [["command"], "Cohere", "var(--text-dim)"],
];

/** Normalize a model key for prefix matching (lowercase, strip non-alphanumerics). */
function normalize(key: string): string {
  return key.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/**
 * Infer the vendor from a model name.
 * Returns `{ vendor, color }`, or `null` if no rule matches.
 */
export function modelVendor(modelKey: string): VendorMeta | null {
  const n = normalize(modelKey);
  for (const [prefixes, vendor, color] of RULES) {
    for (const p of prefixes) {
      if (n.startsWith(p)) {
        return { vendor, color };
      }
    }
  }
  return null;
}

/** Vendor display name → vendor ID (for icon lookup). */
export const VENDOR_NAME_TO_ID: Record<string, string> = {
  "Anthropic": "claude",
  "OpenAI": "codex",
  "Google": "gemini", // fallback
  "深度求索": "deepseek",
  "智谱": "glm",
  "阿里": "qwen",
  "阶跃星辰": "stepfun",
  "月之暗面": "kimi",
  "Meta": "meta",
  "Mistral": "mistral",
  "零一万物": "yi",
  "百川": "baichuan",
  "讯飞星火": "iflytek",
  "腾讯混元": "hunyuan",
  "百度文心": "ernie",
  "字节豆包": "doubao",
  "小米 MiMo": "mimo",
  "火山方舟": "volcengine",
  "MiniMax": "minimax",
  "xAI": "grok",
  "Microsoft": "microsoft",
  "Cohere": "cohere",
};

/** Map a model name to the vendor ID used for icon lookup. */
export function vendorIdForModel(modelKey: string): string | null {
  const mv = modelVendor(modelKey);
  if (!mv) return null;
  return VENDOR_NAME_TO_ID[mv.vendor] ?? null;
}
