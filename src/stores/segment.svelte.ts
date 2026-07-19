// Active popover segment (总览/工具/模型/…). Svelte 5 runes module.

let segment = $state<string>("ov");

export function getSegment(): string {
  return segment;
}

export function setSegment(s: string): void {
  segment = s;
}

export function segmentValue(): string {
  return segment;
}
