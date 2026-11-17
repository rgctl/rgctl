/**
 * ⌘K omnibox.
 *
 * A Radix `Dialog.Portal`, which is the whole reason this app is React: the identical component renders
 * *nothing* through `preact/compat`, throwing `InvalidCharacterError`. Focus trapping, dismissal and
 * ARIA wiring come from the primitive rather than from hand-rolled code.
 *
 * Two modes:
 *   - Symbols: the existing substring search over the loaded function index.
 *   - Ask: server-side semantic retrieval, with graph evidence and a Focus handoff.
 *
 * Ask is not a room and not a lens. It is an entry point: the answer names the subject, and the existing
 * Understand lenses inspect it. That is why this component never draws a graph of its own.
 */
import * as Dialog from "@radix-ui/react-dialog";
import { useDeferredValue, useEffect, useMemo, useState } from "react";
import { searchFunctions, type FunctionEntry, type Manifest } from "./data";
import {
  answerFor,
  askSemantic,
  fetchSemanticStatus,
  formatScore,
  hitLabel,
  shortPath,
  type SemanticHit,
  type SemanticQueryResponse,
  type SemanticStatus,
} from "./ask";
import { Chip, Command } from "./shell";

export function Palette({
  open,
  onOpenChange,
  functions,
  functionsLoading = false,
  manifest,
  onPick,
  onPickKey,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  functions: FunctionEntry[];
  functionsLoading?: boolean;
  manifest: Manifest | null;
  onPick: (entry: FunctionEntry) => void;
  onPickKey: (key: string) => void;
}) {
  const [mode, setMode] = useState<"symbols" | "ask">("symbols");
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);

  const [status, setStatus] = useState<SemanticStatus | null>(null);
  const [statusLoading, setStatusLoading] = useState(false);
  const [result, setResult] = useState<SemanticQueryResponse | null>(null);
  const [lastQuery, setLastQuery] = useState<string | null>(null);
  const [askLoading, setAskLoading] = useState(false);
  const [askError, setAskError] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);
  const [apiMatches, setApiMatches] = useState<FunctionEntry[] | null>(null);

  const deferredQuery = useDeferredValue(query);
  useEffect(() => {
    if (!open || mode !== "symbols") return;
    const q = deferredQuery.trim();
    if (!q) {
      setApiMatches(null);
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      void searchFunctions(q, 40).then((rows) => {
        if (!cancelled) setApiMatches(rows);
      });
    }, 150);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [open, mode, deferredQuery]);

  const localMatches = useMemo(() => {
    const q = deferredQuery.trim().toLowerCase();
    const pool = q
      ? functions.filter(
          (f) => f.name.toLowerCase().includes(q) || (f.file_path ?? "").toLowerCase().includes(q),
        )
      : functions;
    return pool.slice(0, 40);
  }, [functions, deferredQuery]);
  const matches = apiMatches ?? localMatches;

  const resolvable = useMemo(
    () => new Set(functions.map((f) => f.function_id)),
    [functions],
  );

  useEffect(() => setActive(0), [query]);

  // Status is authoritative; manifest.semantic is only a cheap presence flag and can be stale after a
  // late `rgctl semantic index`.
  useEffect(() => {
    if (!open || mode !== "ask" || status !== null || statusLoading) return;
    setStatusLoading(true);
    void fetchSemanticStatus()
      .then(setStatus)
      .catch((err: unknown) =>
        setStatus({
          available: false,
          message: `Could not reach /api/semantic/status: ${String(err)}`,
        }),
      )
      .finally(() => setStatusLoading(false));
  }, [open, mode, status, statusLoading]);

  const answer = result ? answerFor(result) : null;
  const digestMismatch =
    status?.graph_digest != null &&
    manifest?.graph?.digest != null &&
    status.graph_digest !== manifest.graph.digest;
  const topResolvable = answer?.top != null && resolvable.has(answer.top.node_id);
  const blastByAnchor = useMemo(
    () => new Map((result?.expansion?.blast ?? []).map((b) => [b.anchor_node_id, b])),
    [result],
  );

  async function runAsk(text = query) {
    const q = text.trim();
    if (!q || askLoading) return;
    setAskLoading(true);
    setAskError(null);
    try {
      const response = await askSemantic(q, { limit: 20, expand: "blast" });
      setResult(response);
      setLastQuery(response.query);
    } catch (err) {
      setResult(null);
      setAskError(err instanceof Error ? err.message : String(err));
    } finally {
      setAskLoading(false);
    }
  }

  function openHit(hit: SemanticHit) {
    if (!resolvable.has(hit.node_id)) return;
    onPickKey(hit.node_id);
    onOpenChange(false);
  }

  async function copyCommand() {
    const q = lastQuery ?? query.trim();
    await navigator.clipboard.writeText(
      `rgctl -f json semantic query ${JSON.stringify(q)} --limit 20 --expand blast`,
    );
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1500);
  }

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 z-40 bg-black/60" />
        <Dialog.Content
          aria-describedby={undefined}
          className={`fixed left-1/2 top-20 z-50 -translate-x-1/2 rounded-lg border border-white/12 bg-[#12151b] shadow-2xl ${
            mode === "ask" ? "w-[min(960px,94vw)]" : "w-[min(680px,92vw)]"
          }`}
        >
          <Dialog.Title className="sr-only">
            {mode === "ask" ? "Ask the codebase" : "Search symbols"}
          </Dialog.Title>

          <div className="flex items-center gap-1 border-b border-white/8 px-2 py-1.5">
            <span className="mr-1 text-[10px] uppercase tracking-wider text-zinc-600">search</span>
            {(["symbols", "ask"] as const).map((m) => (
              <button
                key={m}
                type="button"
                aria-pressed={mode === m}
                onClick={() => setMode(m)}
                className={`rounded px-2 py-1 text-[11px] ${
                  mode === m
                    ? "bg-white/10 text-zinc-100"
                    : "text-zinc-400 hover:bg-white/[0.05] hover:text-zinc-200"
                }`}
              >
                {m === "symbols" ? "Symbols" : "Ask"}
              </button>
            ))}
            <span className="ml-auto text-[10px] text-zinc-600">
              {mode === "ask" ? "semantic + graph evidence" : "function index"}
            </span>
          </div>

          <form
            onSubmit={(e) => {
              e.preventDefault();
              if (mode === "ask") void runAsk();
            }}
            className="flex items-center gap-2 border-b border-white/8 px-3 py-2"
          >
            <input
              autoFocus
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "ArrowDown") {
                  e.preventDefault();
                  if (mode === "symbols") setActive((i) => Math.min(i + 1, matches.length - 1));
                }
                if (e.key === "ArrowUp") {
                  e.preventDefault();
                  if (mode === "symbols") setActive((i) => Math.max(i - 1, 0));
                }
                if (e.key === "Enter" && mode === "symbols" && matches[active]) {
                  e.preventDefault();
                  onPick(matches[active]);
                }
              }}
              placeholder={
                mode === "ask"
                  ? "e.g. where is checkout pricing calculated?"
                  : "Search symbols, or paste a stable key…"
              }
              className="w-full bg-transparent text-[13px] text-zinc-100 outline-none placeholder:text-zinc-600"
            />
            {mode === "ask" ? (
              <button
                type="submit"
                disabled={askLoading || !query.trim()}
                className="whitespace-nowrap rounded border border-sky-400/30 bg-sky-500/10 px-2 py-1 text-[11px] text-sky-200 disabled:opacity-40"
              >
                {askLoading ? "Asking…" : "Ask →"}
              </button>
            ) : (
              <span className="rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-zinc-500">
                esc
              </span>
            )}
          </form>

          {mode === "symbols" ? (
            <SymbolResults
              matches={matches}
              active={active}
              total={functions.length}
              loading={functionsLoading}
              onActive={setActive}
              onPick={onPick}
            />
          ) : (
            <AskResults
              status={status}
              statusLoading={statusLoading}
              manifest={manifest}
              digestMismatch={digestMismatch}
              result={result}
              answer={answer}
              lastQuery={lastQuery}
              askLoading={askLoading}
              askError={askError}
              topResolvable={topResolvable}
              resolvable={resolvable}
              blastByAnchor={blastByAnchor}
              copied={copied}
              onSuggestion={(text) => {
                setQuery(text);
                void runAsk(text);
              }}
              onOpenTop={() => answer?.top && openHit(answer.top)}
              onOpenHit={openHit}
              onCopyCommand={() => void copyCommand()}
            />
          )}

          <div className="flex items-center justify-between border-t border-white/8 px-3 py-1.5 text-[10px] text-zinc-600">
            <span>
              {mode === "ask"
                ? "⏎ ask · results open the strongest resolvable hit"
                : "↑↓ move · ⏎ focus"}
            </span>
            <span>
              {mode === "ask"
                ? "function scope · rgctl serve"
                : `${matches.length} of ${functions.length} candidates`}
            </span>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function SymbolResults({
  matches,
  active,
  total,
  loading,
  onActive,
  onPick,
}: {
  matches: FunctionEntry[];
  active: number;
  total: number;
  loading: boolean;
  onActive: (index: number) => void;
  onPick: (entry: FunctionEntry) => void;
}) {
  return (
    <div className="max-h-[52vh] overflow-y-auto p-1">
      {matches.length === 0 && (
        <p className="px-2 py-3 text-[12px] text-zinc-500">
          {loading && total === 0
            ? "Loading the function index…"
            : `No function matches. The bundle carries ${total} function${total === 1 ? "" : "s"}.`}
          {!loading && total === 0 && " This bundle has no CFG/slice index; use Ask for semantic results."}
        </p>
      )}
      {matches.map((f, i) => (
        <button
          key={f.function_id}
          type="button"
          onMouseEnter={() => onActive(i)}
          onClick={() => onPick(f)}
          className={`flex w-full items-center gap-3 rounded px-2 py-1.5 text-left ${
            i === active ? "bg-white/8" : "hover:bg-white/[0.04]"
          }`}
        >
          <span className="min-w-0 flex-1">
            <span className="block truncate text-[12px] text-zinc-100">{f.name}</span>
            <span className="block truncate font-mono text-[10px] text-zinc-500">
              {f.file_path ?? "—"}
            </span>
          </span>
          <span className="shrink-0 font-mono text-[10px] text-zinc-600">
            {f.pdg_nodes ?? "—"} pdg · {f.block_count ?? "—"} blk
          </span>
        </button>
      ))}
    </div>
  );
}

function AskResults({
  status,
  statusLoading,
  manifest,
  digestMismatch,
  result,
  answer,
  lastQuery,
  askLoading,
  askError,
  topResolvable,
  resolvable,
  blastByAnchor,
  copied,
  onSuggestion,
  onOpenTop,
  onOpenHit,
  onCopyCommand,
}: {
  status: SemanticStatus | null;
  statusLoading: boolean;
  manifest: Manifest | null;
  digestMismatch: boolean;
  result: SemanticQueryResponse | null;
  answer: ReturnType<typeof answerFor> | null;
  lastQuery: string | null;
  askLoading: boolean;
  askError: string | null;
  topResolvable: boolean;
  resolvable: Set<string>;
  blastByAnchor: Map<string, { direct_callers: number; impact_zone: number }>;
  copied: boolean;
  onSuggestion: (text: string) => void;
  onOpenTop: () => void;
  onOpenHit: (hit: SemanticHit) => void;
  onCopyCommand: () => void;
}) {
  return (
    <div className="max-h-[66vh] overflow-y-auto p-3">
      {statusLoading && (
        <p className="mb-2 text-[11px] text-zinc-500">Checking the semantic index…</p>
      )}

      {!statusLoading && status && !status.available && (
        <div className="mb-2 rounded border border-amber-500/25 bg-amber-500/[0.04] p-2.5">
          <div className="flex items-center gap-2">
            <Chip tone="amber">not available</Chip>
            <span className="text-[12px] text-zinc-200">{status.message ?? "Semantic index is not ready."}</span>
          </div>
          <Command>rgctl semantic index</Command>
          <p className="mt-1 text-[10px] text-zinc-500">
            The index is served by `rgctl serve`; a static file server cannot answer this request.
          </p>
        </div>
      )}

      {!statusLoading && status?.available && (
        <div className="mb-2 flex flex-wrap items-center gap-2 text-[10px] text-zinc-500">
          <Chip tone="green">ready</Chip>
          <span className="font-mono">{status.model_id ?? "model unknown"}</span>
          {status.dimensions != null && <span>{status.dimensions}d</span>}
          {status.functions_indexed != null && (
            <span>{status.functions_indexed.toLocaleString()} functions</span>
          )}
          {status.graph_digest && (
            <span className="font-mono" title={status.graph_digest}>
              digest {status.graph_digest.slice(0, 12)}
            </span>
          )}
        </div>
      )}

      {digestMismatch && (
        <div className="mb-2 rounded border border-amber-500/25 bg-amber-500/[0.04] p-2 text-[11px] text-amber-200">
          The semantic index was built for a different graph snapshot than this bundle. Its results may
          describe symbols that no longer exist. Rebuild with `rgctl semantic index`.
        </div>
      )}

      {answer == null && !askLoading && (
        <div className="rounded border border-white/8 bg-white/[0.02] p-3">
          <p className="text-[11px] leading-relaxed text-zinc-400">
            Ask the codebase in natural language. Retrieval finds the functions; graph expansion supplies
            the callers and impact-zone evidence. This is not an LLM answer — every line is traceable to
            the index and the bundle.
          </p>
          <div className="mt-2 flex flex-wrap gap-1.5">
            {["checkout pricing", "authentication", "database migration", "where is configuration loaded?"].map(
              (example) => (
                <button
                  key={example}
                  type="button"
                  onClick={() => onSuggestion(example)}
                  className="rounded border border-white/10 bg-white/[0.03] px-2 py-1 text-[10px] text-zinc-400 hover:border-sky-400/30 hover:text-sky-200"
                >
                  {example}
                </button>
              ),
            )}
          </div>
          {manifest?.semantic?.available === false && (
            <p className="mt-2 text-[10px] text-zinc-600">
              The bundle manifest does not advertise a semantic index; the live API status is authoritative.
            </p>
          )}
        </div>
      )}

      {askLoading && <p className="mb-2 text-[11px] text-zinc-400">Retrieving and expanding…</p>}
      {askError && (
        <div className="mb-2 rounded border border-rose-500/25 bg-rose-500/[0.05] p-2 text-[11px] text-rose-200">
          {askError}
        </div>
      )}

      {answer && result && (
        <div className="space-y-2">
          <div className="rounded-lg border border-sky-400/25 bg-sky-500/[0.04] p-3">
            <div className="flex flex-wrap items-center gap-2">
              <Chip tone="blue">answer</Chip>
              <span className="text-[12px] text-zinc-100">{answer.headline}</span>
              {answer.top?.ranking === "fusion" && <Chip>fusion</Chip>}
            </div>
            <p className="mt-1.5 text-[12px] leading-relaxed text-zinc-300">{answer.summary}</p>
            <div className="mt-2 flex flex-wrap items-center gap-2">
              {topResolvable ? (
                <button
                  type="button"
                  onClick={onOpenTop}
                  className="rounded border border-sky-400/30 bg-sky-500/10 px-2 py-1 text-[11px] text-sky-200 hover:bg-sky-500/15"
                >
                  Open in Understand
                </button>
              ) : (
                <span className="text-[10px] text-zinc-500">
                  Evidence only — this hit is outside the bundle&apos;s function index.
                </span>
              )}
              <button
                type="button"
                onClick={onCopyCommand}
                className="rounded border border-white/10 px-2 py-1 text-[10px] text-zinc-400 hover:text-zinc-200"
              >
                {copied ? "Copied" : "Copy CLI"}
              </button>
            </div>
          </div>

          <div className="overflow-x-auto rounded border border-white/8">
            <table className="w-full text-left">
              <thead className="border-b border-white/8 text-[10px] uppercase tracking-wider text-zinc-600">
                <tr>
                  <th className="px-2 py-1.5 font-medium">#</th>
                  <th className="px-2 py-1.5 font-medium">Symbol</th>
                  <th className="px-2 py-1.5 font-medium">File</th>
                  <th className="px-2 py-1.5 text-right font-medium">Score</th>
                  <th className="px-2 py-1.5 text-right font-medium">Callers</th>
                  <th className="px-2 py-1.5 text-right font-medium">Blast</th>
                </tr>
              </thead>
              <tbody>
                {result.hits.map((hit, i) => {
                  const blast = blastByAnchor.get(hit.node_id);
                  const canOpen = resolvable.has(hit.node_id);
                  return (
                    <tr
                      key={hit.node_id}
                      className={`border-b border-white/5 last:border-0 ${
                        canOpen ? "cursor-pointer hover:bg-white/[0.04]" : ""
                      }`}
                      onClick={() => canOpen && onOpenHit(hit)}
                    >
                      <td className="px-2 py-1.5 font-mono text-[10px] text-zinc-600">{i + 1}</td>
                      <td className="max-w-[260px] px-2 py-1.5">
                        <span className="block truncate text-[11px] text-zinc-200" title={hitLabel(hit)}>
                          {hitLabel(hit)}
                        </span>
                        {!canOpen && (
                          <span className="text-[9px] text-amber-300/80">evidence only</span>
                        )}
                      </td>
                      <td className="max-w-[240px] px-2 py-1.5 font-mono text-[10px] text-zinc-500">
                        <span className="block truncate" title={hit.file_path ?? undefined}>
                          {shortPath(hit.file_path)}
                        </span>
                      </td>
                      <td className="px-2 py-1.5 text-right font-mono text-[10px] text-zinc-300">
                        {formatScore(hit.score)}
                      </td>
                      <td className="px-2 py-1.5 text-right font-mono text-[10px] text-zinc-500">
                        {blast?.direct_callers ?? "—"}
                      </td>
                      <td className="px-2 py-1.5 text-right font-mono text-[10px] text-zinc-500">
                        {blast?.impact_zone ?? "—"}
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>

          {result.hits.length === 0 && (
            <p className="rounded border border-white/8 bg-white/[0.02] p-3 text-[11px] text-zinc-400">
              No function matched this query. Try a broader phrase or a different vocabulary; an empty
              result is not evidence that the code does not exist.
            </p>
          )}

          <div className="rounded border border-white/8 bg-black/30 p-2 text-[10px] text-zinc-500">
            <div className="flex flex-wrap items-center gap-2">
              <span className="font-mono">{result.model_id}</span>
              <span>{result.dimensions}d</span>
              <span>
                query {JSON.stringify(lastQuery ?? result.query)} · limit 20 · expand blast
              </span>
            </div>
            <Command>
              {`rgctl -f json semantic query ${JSON.stringify(lastQuery ?? result.query)} --limit 20 --expand blast`}
            </Command>
          </div>
        </div>
      )}
    </div>
  );
}
