import { bundleDataUrl } from "./bundleUrl";
import type { SliceBundlePayload } from "./types";

const sourceTextCache = new Map<string, string>();

/** Resolve display source for a slice bundle (inline v1 or deduplicated v2). */
export async function resolveSliceSource(
  bundle: SliceBundlePayload,
): Promise<string> {
  if (bundle.source) {
    return bundle.source;
  }
  if (!bundle.source_id) {
    return "";
  }
  const cached = sourceTextCache.get(bundle.source_id);
  if (cached !== undefined) {
    return cached;
  }
  const res = await fetch(bundleDataUrl(`sources/${bundle.source_id}.txt`));
  if (!res.ok) {
    throw new Error(`sources/${bundle.source_id}.txt: HTTP ${res.status}`);
  }
  const text = await res.text();
  sourceTextCache.set(bundle.source_id, text);
  return text;
}

/** Optional function body excerpt when start/end lines are known. */
export function excerptSource(
  source: string,
  startLine?: number | null,
  endLine?: number | null,
): string {
  if (!startLine || !endLine || startLine < 1) {
    return source;
  }
  const lines = source.split("\n");
  const start = Math.max(0, startLine - 1);
  const end = Math.min(lines.length, endLine);
  return lines.slice(start, end).join("\n");
}
