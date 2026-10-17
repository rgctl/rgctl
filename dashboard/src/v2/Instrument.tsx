/**
 * The Instrument layer — persistent, and never a destination.
 *
 * Coverage is computed from what the bundle can *evidence* (`analysis.*_available`, `semantic.available`,
 * `phases`), per the spec's rule that a capability which cannot be evidenced is not shown as covered. The
 * design promised per-language support tiers here; that data is not in the bundle, so it is reported as
 * unavailable rather than approximated with a dot count that looks like a measurement.
 */
import { capabilities, type Manifest } from "./data";

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
  const filled = caps.filter((c) => c.available).length;
  const digest = manifest?.graph?.digest ?? manifest?.export_fingerprint;

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

        <span
          className="flex items-center gap-1"
          title={caps.map((c) => `${c.available ? "✓" : "✗"} ${c.label}`).join("\n")}
        >
          {caps.map((c) => (
            <span
              key={c.label}
              className={`h-1.5 w-1.5 rounded-full ${c.available ? "bg-emerald-400" : "bg-zinc-700"}`}
            />
          ))}
          <span className="ml-1 text-[10px] text-zinc-500">
            {filled}/{caps.length} capabilities · per-language tiers not in this bundle
          </span>
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
