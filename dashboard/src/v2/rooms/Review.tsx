/**
 * The Review room — "what changed, and what does it touch?"
 *
 * It renders the document `rgctl review export` composes from a pair of revisions. It performs no
 * analysis of its own: every figure here is the artifact's, and the command that produced it is shown
 * with them (design rule 2 — every number carries a receipt).
 *
 * Three rules are structural rather than decorative:
 *
 *   1. **Absence says why.** No artifact, an unevaluated policy, an empty timeline and a truncated report
 *      are four different facts. An empty policy section rendered as "no violations" would be a lie, and
 *      it is the easiest lie for a review surface to tell.
 *   2. **Freshness is stated.** The artifact describes a revision *pair*; the bundle describes one graph.
 *      When they disagree the room says so above the numbers rather than presenting them as current.
 *   3. **Identity is resolved, never guessed.** The artifact keys symbols by an analysis cache key, so
 *      identity comes from joining file + name against the index. An ambiguous name renders without a
 *      handoff instead of silently carrying the wrong function.
 */
import { useQuery } from "@tanstack/react-query";
import { loadFunctions, loadManifest, type FunctionEntry } from "../data";
import { Chip, Command, Panel, Stat, Unavailable } from "../shell";
import {
  absenceNotes,
  formatGeneratedAt,
  identityOf,
  loadWorkflow,
  PRODUCE_COMMAND,
  truncationOf,
  type CallEdgeDelta,
  type ReviewSymbol,
} from "../workflow";

export function ReviewRoom({ onFocus }: { onFocus: (functionId: string) => void }) {
  const manifest = useQuery({ queryKey: ["manifest"], queryFn: loadManifest });
  const functions = useQuery({ queryKey: ["functions"], queryFn: loadFunctions });
  const workflow = useQuery({
    // Keyed on the bundle's graph digest: a different graph is a different freshness answer.
    queryKey: ["workflow", manifest.data?.graph?.digest ?? "pending"],
    queryFn: () => loadWorkflow(manifest.data ?? null),
    enabled: manifest.isFetched,
  });

  if (workflow.isPending) {
    return (
      <Panel title="Review" subtitle="reading the workflow artifact">
        <p className="text-[12px] text-zinc-400">Loading…</p>
      </Panel>
    );
  }

  const state = workflow.data;
  if (!state || state.kind === "empty") {
    return (
      <Panel title="Review" subtitle="what changed, and what does it touch?">
        <Unavailable
          tag="no artifact in this bundle"
          what="No review artifact was exported with this dashboard"
          why="The room renders the document `rgctl review export` composes from a pair of revisions — a base and a head. This bundle carries none, so there is nothing to show. It is not an empty review."
        />
        <Command>{PRODUCE_COMMAND}</Command>
        <p className="mt-2 text-[11px] leading-relaxed text-zinc-500">
          Producing one needs a <span className="text-zinc-400">base artifact</span> — a second indexed
          revision at <code className="font-mono">.rgctl-base/</code>, or passed with{" "}
          <code className="font-mono">--base-artifact</code> — and git refs on this repository. The export
          must then be run <span className="text-zinc-400">before</span> the dashboard is re-exported with{" "}
          <code className="font-mono">discover --with-dashboard</code>, which copies it into this bundle.
        </p>
      </Panel>
    );
  }

  if (state.kind === "unreadable") {
    return (
      <Panel title="Review" subtitle="what changed, and what does it touch?">
        <Unavailable
          tag="unreadable"
          what="A review artifact is present but could not be read"
          why={`${state.reason}. The bundle records one, so this is a broken artifact rather than an absent one.`}
        />
      </Panel>
    );
  }

  const { artifact, freshness } = state.workflow;
  const paths = artifact.review_paths;
  const summary = paths?.change_summary;
  const symbols = paths?.symbols ?? [];
  const cut = truncationOf([artifact.truncation, paths?.truncation, ...symbols.map((s) => s.truncation)]);
  const notes = absenceNotes(artifact);
  const generated = formatGeneratedAt(artifact.generated_at);
  const commands = artifact.provenance?.commands ?? [];

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {freshness !== "current" && (
        <Unavailable
          tag={freshness === "different-revision" ? "different revision" : "freshness unknown"}
          what={
            freshness === "different-revision"
              ? "This artifact describes a different revision than the graph in this bundle"
              : "Whether this artifact matches the browsed graph could not be established"
          }
          why={
            freshness === "different-revision"
              ? "The digests disagree, so the figures below are about another revision pair. They are worth reading as history, not as the current state of this graph."
              : "The artifact or the manifest is missing a graph digest, so no comparison was possible. That is not the same as a match."
          }
        />
      )}

      <Panel
        title="Review"
        subtitle={generated ? `artifact generated ${generated}` : "composed review artifact"}
        right={
          freshness === "current" ? (
            <Chip tone="green">same revision</Chip>
          ) : (
            <Chip tone="amber">{freshness === "different-revision" ? "other revision" : "unverified"}</Chip>
          )
        }
      >
        <div className="flex flex-wrap items-start gap-x-8 gap-y-3">
          <Stat
            label="changed symbols"
            value={summary?.changed_symbols ?? "—"}
            hint={cut.includes("symbols") ? "truncated — not the full set" : undefined}
          />
          <Stat label="files in scope" value={summary?.files_in_scope ?? "—"} />
          <Stat label="call edges added" value={summary?.call_edges?.added ?? "—"} />
          <Stat label="retargeted" value={summary?.call_edges?.retargeted ?? "—"} />
          <Stat label="removed" value={summary?.call_edges?.removed ?? "—"} />
          {summary?.unscored_files ? <Stat label="unscored files" value={summary.unscored_files} /> : null}
        </div>
        {cut.length > 0 && (
          <p className="mt-3 rounded border border-amber-500/25 bg-amber-500/[0.04] px-2 py-1.5 text-[11px] text-amber-200">
            The report was truncated ({cut.join(", ")}), so these lists are incomplete by construction —
            not a complete review of a small change.
          </p>
        )}
        {!summary && (
          <p className="mt-3 text-[11px] text-zinc-400">
            The artifact carries no change summary, so there are no totals to show. Its sections are listed
            below as they were recorded.
          </p>
        )}
      </Panel>

      <Panel
        title="Changed symbols"
        subtitle={symbols.length > 0 ? `${symbols.length} with call-path detail` : undefined}
      >
        {symbols.length === 0 ? (
          <p className="text-[11px] text-zinc-400">
            The artifact lists no symbols with call-path detail. That is the artifact's own content, not a
            failure to load it.
          </p>
        ) : (
          <ul className="flex flex-col divide-y divide-white/6">
            {symbols.map((symbol, i) => (
              <SymbolRow
                key={symbol.stable_key ?? `${symbol.name}-${i}`}
                symbol={symbol}
                functions={functions.data ?? []}
                onFocus={onFocus}
              />
            ))}
          </ul>
        )}
      </Panel>

      {notes.length > 0 && (
        <Panel title="Not evaluated" subtitle="sections the artifact says it did not produce">
          <ul className="flex flex-col gap-1.5">
            {notes.map((note) => (
              <li key={note.section} className="text-[11px] leading-relaxed text-zinc-400">
                <span className="font-mono text-zinc-300">{note.section}</span> — {note.reason}
              </li>
            ))}
          </ul>
        </Panel>
      )}

      {commands.length > 0 && (
        <Panel title="Receipt" subtitle="the command that produced this document">
          {commands.map((command) => (
            <Command key={command}>{command}</Command>
          ))}
        </Panel>
      )}
    </div>
  );
}

function SymbolRow({
  symbol,
  functions,
  onFocus,
}: {
  symbol: ReviewSymbol;
  functions: FunctionEntry[];
  onFocus: (functionId: string) => void;
}) {
  const identity = identityOf(symbol, functions);
  const file = symbol.head?.file ?? "";
  const line = symbol.head?.start_line;
  const deltas = symbol.path_delta ?? [];

  return (
    <li className="flex flex-col gap-1.5 py-2 first:pt-0 last:pb-0">
      <div className="flex flex-wrap items-baseline gap-2">
        <span className="font-mono text-[12px] text-zinc-100">{symbol.name ?? "(unnamed)"}</span>
        {symbol.kind && <span className="text-[10px] uppercase tracking-wider text-zinc-500">{symbol.kind}</span>}
        <span className="truncate font-mono text-[10px] text-zinc-500" title={file}>
          {shorten(file)}
          {line ? `:${line}` : ""}
        </span>
        <span className="ml-auto flex items-center gap-2">
          {deltas.length > 0 && (
            <span className="text-[10px] text-zinc-500">
              {deltas.length} call-path {deltas.length === 1 ? "change" : "changes"}
            </span>
          )}
          {identity.kind === "unique" ? (
            <button
              type="button"
              onClick={() => onFocus(identity.functionId)}
              className="rounded border border-white/12 bg-white/[0.03] px-2 py-0.5 text-[11px] text-zinc-300 hover:border-sky-400/40 hover:text-sky-200"
            >
              carry →
            </button>
          ) : identity.kind === "ambiguous" ? (
            <span
              className="text-[10px] text-amber-300/90"
              title={`${identity.count} indexed functions share this name in this file — the artifact records no node id, so carrying one would be a guess`}
            >
              ambiguous ({identity.count}) — no handoff
            </span>
          ) : (
            <span className="text-[10px] text-zinc-500" title="Not found in the indexed graph for this bundle">
              not in this graph
            </span>
          )}
        </span>
      </div>
      {deltas.length > 0 && (
        <ul className="flex flex-col gap-0.5">
          {deltas.map((delta, i) => (
            <li key={`${delta.kind}-${delta.from}-${delta.to}-${i}`} className="flex items-center gap-2">
              <DeltaChip kind={delta.kind} />
              <span className="font-mono text-[11px] text-zinc-400">
                {delta.from} <span className="text-zinc-600">→</span> {delta.to}
              </span>
            </li>
          ))}
        </ul>
      )}
    </li>
  );
}

function DeltaChip({ kind }: { kind: CallEdgeDelta["kind"] }) {
  const tone = kind === "added" ? "green" : kind === "removed" ? "amber" : "zinc";
  return <Chip tone={tone}>{kind}</Chip>;
}

/** Absolute paths are what the artifact records; the tail is what identifies them on screen. */
function shorten(path: string): string {
  if (!path) return "(no path)";
  const parts = path.split("/").filter(Boolean);
  return parts.length <= 3 ? path : `…/${parts.slice(-3).join("/")}`;
}
