/**
 * The Instrument layer — persistent, and never a destination.
 *
 * Coverage dots are per-language support tiers from `languages.toml`, filtered to languages whose
 * files appear in this bundle. Analysis capabilities remain a separate tooltip (CFG, slices, …).
 */
import { useQuery } from "@tanstack/react-query";
import { useMemo } from "react";
import {
  capabilities,
  languageCoverageFromPaths,
  loadSourceIndex,
  type Manifest,
} from "./data";

function freshness(generatedAt?: string): string {
  const seconds = Number(generatedAt);
  if (!Number.isFinite(seconds) || seconds <= 0) return "unknown";
  const age = Math.max(0, Math.floor(Date.now() / 1000) - seconds);
  if (age < 90) return `${age}s ago`;
  if (age < 5400) return `${Math.round(age / 60)}m ago`;
  if (age < 172800) return `${Math.round(age / 3600)}h ago`;
  return `${Math.round(age / 86400)}d ago`;
}

export function Instrument({
  manifest,
  focusLabel,
  onClearFocus,
}: {
  manifest: Manifest | null;
  focusLabel: string | null;
  onClearFocus: () => void;
}) {
  const caps = capabilities(manifest);
  const digest = manifest?.graph?.digest ?? manifest?.export_fingerprint;
  const sources = useQuery({ queryKey: ["sources-index"], queryFn: loadSourceIndex });
  const coverage = useMemo(
    () => languageCoverageFromPaths((sources.data ?? []).map((f) => f.file_path)),
    [sources.data],
  );
  const tier1 = coverage.languages.filter((l) => l.tier === 1).length;
  const tier2 = coverage.languages.filter((l) => l.tier === 2).length;

  const coverageTitle = [
    ...coverage.languages.map((l) => `T${l.tier} ${l.id} · ${l.fileCount} file(s)`),
    coverage.unstatedFiles > 0
      ? `${coverage.unstatedFiles} file(s) with no recorded language tier (not shown as covered)`
      : null,
    "—",
    ...caps.map((c) => `${c.available ? "✓" : "✗"} ${c.label}`),
  ]
    .filter(Boolean)
    .join("\n");

  return (
    <header className="shrink-0 border-b border-white/8 bg-[#0e1116] px-3 py-2">
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1">
        <span className="flex items-center gap-2">
          <span className="text-[13px] font-semibold tracking-tight text-sky-300">rgctl</span>
          <span className="text-zinc-600">/</span>
          <span className="text-[12px] text-zinc-300">
            {manifest?.graph?.node_count ? `${manifest.graph.node_count.toLocaleString()} nodes` : "no bundle"}
          </span>
        </span>

        <span className="flex items-center gap-1" title={coverageTitle}>
          {coverage.languages.length === 0 && !sources.isLoading ? (
            <span className="text-[10px] text-zinc-500">no language coverage in this bundle</span>
          ) : (
            <>
              {coverage.languages.map((l) => (
                <span
                  key={l.id}
                  className={`h-1.5 w-1.5 rounded-full ${l.tier === 1 ? "bg-emerald-400" : "bg-sky-400"}`}
                  title={`T${l.tier} ${l.id}`}
                />
              ))}
              <span className="ml-1 text-[10px] text-zinc-500">
                {coverage.languages.length} language
                {coverage.languages.length === 1 ? "" : "s"}
                {tier1 + tier2 > 0 ? ` · T1 ${tier1}` : ""}
                {tier2 > 0 ? ` · T2 ${tier2}` : ""}
                {coverage.unstatedFiles > 0 ? ` · ${coverage.unstatedFiles} unstated` : ""}
              </span>
            </>
          )}
        </span>

        {digest && (
          <span className="font-mono text-[11px] text-zinc-400" title={manifest?.graph?.digest}>
            {digest.slice(0, 12)}
          </span>
        )}
        <span className="text-[11px] text-zinc-500">indexed {freshness(manifest?.generated_at)}</span>
        {manifest?.dashboard_version && (
          <span className="text-[11px] text-zinc-600">v{manifest.dashboard_version}</span>
        )}

        <span className="ml-auto flex items-center gap-2">
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">focus</span>
          {focusLabel ? (
            <span className="flex items-center gap-1.5 rounded border border-sky-400/30 bg-sky-500/10 px-2 py-0.5">
              <span className="text-[11px] text-sky-100">{focusLabel}</span>
              <button
                type="button"
                onClick={onClearFocus}
                aria-label="clear focus"
                className="text-sky-300/70 hover:text-sky-100"
              >
                ✕
              </button>
            </span>
          ) : (
            <span className="rounded border border-white/10 px-2 py-0.5 text-[11px] text-zinc-500">none</span>
          )}
        </span>
      </div>
    </header>
  );
}
