/**
 * Load PDG / dataflow detail for the active function.
 *
 * Prefers `slice/<id>.json` when present; falls back to HTTP Range on
 * `slice.record_{index,data}.bin` when `manifest.capabilities.slice_detail_archive`
 * is set (or when the per-file JSON 404s and a record pack exists).
 */
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { bundleUrl, type Manifest } from "../data";
import type { DfgDetail } from "../rooms/DfgGraph";
import {
  isSliceArchiveOnly,
  loadSliceIndexMeta,
  loadSliceJsonFromArchive,
  type SliceIndexMeta,
} from "../cfgArchive";

export type DfgState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; dfg: DfgDetail }
  | { kind: "absent"; reason: string }
  | { kind: "failed"; reason: string };

interface SliceBundle {
  function_id: string;
  name: string;
  pdg?: {
    nodes?: Array<{
      id: string;
      line: number;
      label: string;
      kind?: string;
      defined?: string[];
      used?: string[];
    }>;
    edges?: Array<{
      source: string;
      target: string;
      kind: string;
      variable?: string | null;
    }>;
  };
}

function toDfg(raw: SliceBundle): DfgDetail | null {
  const nodes = raw.pdg?.nodes ?? [];
  const edges = raw.pdg?.edges ?? [];
  if (nodes.length === 0) return null;
  return {
    function_id: raw.function_id,
    name: raw.name,
    nodes,
    edges,
  };
}

async function loadFromPerFile(
  functionId: string,
  detailDir: string,
  fetchImpl: typeof fetch,
): Promise<DfgDetail | null> {
  const res = await fetchImpl(bundleUrl(`${detailDir}/${functionId}.json`), {
    cache: "no-store",
  });
  if (!res.ok) return null;
  const raw = (await res.json()) as SliceBundle;
  return toDfg(raw);
}

async function loadFromArchive(
  functionId: string,
  meta: SliceIndexMeta | null,
  fetchImpl: typeof fetch,
): Promise<DfgDetail | null> {
  const bytes = await loadSliceJsonFromArchive(functionId, meta, fetchImpl);
  if (!bytes) return null;
  const text = new TextDecoder().decode(bytes);
  const raw = JSON.parse(text) as SliceBundle;
  return toDfg(raw);
}

function readBootstrappedManifest(): Manifest | null {
  if (typeof document === "undefined") return null;
  const el = document.getElementById("rgctl-manifest");
  if (!el?.textContent) return null;
  try {
    return JSON.parse(el.textContent) as Manifest;
  } catch {
    return null;
  }
}

export async function loadDfgDetail(
  functionId: string,
  detailDir = "slice",
  fetchImpl: typeof fetch = fetch,
  manifest: Manifest | null = null,
): Promise<DfgDetail | null> {
  const effectiveManifest = manifest ?? readBootstrappedManifest();
  const preferArchive = Boolean(effectiveManifest?.capabilities?.slice_detail_archive);
  let sliceMeta: SliceIndexMeta | null = null;

  if (preferArchive) {
    sliceMeta = {
      detail_mode: "archive_only",
      record_index_path: effectiveManifest?.slice_record_index_path ?? "slice.record_index.bin",
      record_data_path: effectiveManifest?.slice_record_data_path ?? "slice.record_data.bin",
    };
    const fromArchive = await loadFromArchive(functionId, sliceMeta, fetchImpl);
    if (fromArchive) return fromArchive;
  }

  // When manifest was not passed and no bootstrap script is present, check cached
  // `slice_index.json` metadata before probing `slice/<id>.json` so archive_only
  // bundles do not emit a 404 on every function selection.
  if (!effectiveManifest) {
    sliceMeta = await loadSliceIndexMeta(fetchImpl);
    if (isSliceArchiveOnly(sliceMeta)) {
      return loadFromArchive(functionId, sliceMeta, fetchImpl);
    }
  }

  const fromFile = await loadFromPerFile(functionId, detailDir, fetchImpl);
  if (fromFile) return fromFile;

  // Per-file missing — try archive via slice_index.json even without manifest flag
  // (covers older shells / warm reloads that only have the record pack).
  sliceMeta = sliceMeta ?? (await loadSliceIndexMeta(fetchImpl));
  if (isSliceArchiveOnly(sliceMeta) || sliceMeta?.record_index_path) {
    return loadFromArchive(functionId, sliceMeta, fetchImpl);
  }
  return null;
}

export function useDfgDetail(
  functionId: string | null,
  enabled: boolean,
  detailDir = "slice",
  manifest: Manifest | null = null,
): DfgState {
  const [state, setState] = useState<DfgState>({ kind: "idle" });
  const queryClient = useQueryClient();
  const resolvedManifest =
    manifest ?? queryClient.getQueryData<Manifest | null>(["manifest"]) ?? null;

  useEffect(() => {
    if (!enabled) {
      setState({ kind: "idle" });
      return;
    }
    if (!functionId) {
      setState({
        kind: "absent",
        reason: "No function is selected, so there is no DFG sidecar to load.",
      });
      return;
    }
    let cancelled = false;
    setState({ kind: "loading" });
    loadDfgDetail(functionId, detailDir, fetch, resolvedManifest)
      .then((dfg) => {
        if (cancelled) return;
        setState(
          dfg
            ? { kind: "ready", dfg }
            : {
                kind: "absent",
                reason: `No PDG detail for \`${functionId}\` (checked slice JSON and record pack).`,
              },
        );
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setState({ kind: "failed", reason: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [functionId, enabled, detailDir, resolvedManifest]);

  return state;
}
