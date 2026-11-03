/**
 * One symbol, read: its source and (optionally) its control-flow graph.
 *
 * Extracted from the **CFG** lens because a second caller arrived — the Architecture room opens this beside the
 * universe when you double-click a member — and because the alternative was the exact failure the CFG lens
 * already documents: two panes claiming to be "the same reader" while one of them renders nothing. There is one
 * reader; the framings are props.
 *
 * ## The framings
 *
 * | variant | what it draws | who asks for it |
 * |---|---|---|
 * | `source` | the function's lines, nothing beside them | the drawer before it is maximized; this is what the retired Code lens was |
 * | `source-and-graph` | source beside the control-flow graph, bound both ways | the CFG lens; the maximized drawer |
 * | `graph` | the graph at full width | the CFG lens with `show source` off; what the retired CFG·PDG lens was |
 *
 * ## The binding
 *
 * Selecting a block highlights its lines and scrolls to them; clicking a line selects the block that covers it.
 * Both directions matter: with one-way binding the graph is authoritative and the code is a display, and reading
 * does not work that way. In `source` there is no graph, so no row is clickable and no row claims a block it
 * cannot show — the absence is the design, not an oversight.
 *
 * Out of scope in every framing, deliberately: no file tree, no editing, no go-to-definition. This is a view of
 * a symbol, not an IDE.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { relative, repoRoot, type CfgBlock, type FunctionEntry } from "../data";
import { useCfgDetail } from "../useCfgDetail";
import { BlockDetail } from "./BlockDetail";
import {
  CfgGraph,
  frontierBlockIds,
  hasLines,
  maxDominatorDepth,
} from "./CfgGraph";
import { SourcePane, cfgSpan } from "./SourcePane";
import { Chip, Panel, Stat, Unavailable } from "../shell";

export type SymbolVariant = "source" | "source-and-graph" | "graph";

export function SymbolReader({
  entry,
  variant,
  headerRight,
  footer,
}: {
  entry: FunctionEntry;
  variant: SymbolVariant;
  /** Extra controls in the graph panel's header, beside the frontier toggle. */
  headerRight?: ReactNode;
  /** Below the graph — the CFG lens puts its CLI command here. */
  footer?: ReactNode;
}) {
  const cfgState = useCfgDetail(entry.function_id);
  const cfg = cfgState.kind === "ready" ? cfgState.cfg : null;
  const [selectedBlock, setSelectedBlock] = useState<number | null>(null);
  const [prefix, setPrefix] = useState("");
  const [showFrontier, setShowFrontier] = useState(true);

  const wantsSource = variant !== "graph";
  const wantsGraph = variant !== "source";

  // A block id from the previous subject means nothing in this one, so the selection does not travel.
  useEffect(() => {
    let cancelled = false;
    setSelectedBlock(null);
    void repoRoot().then((p) => {
      if (!cancelled) setPrefix(p);
    });
    return () => {
      cancelled = true;
    };
  }, [entry.function_id, entry.file_path]);

  /**
   * Line -> block, the reverse of the block -> lines binding.
   *
   * When several blocks cover a line (nested or synthetic spans), the *smallest* span wins, because that is the
   * block the line most specifically belongs to. Blocks with no source lines are excluded outright rather than
   * matching line 0 by accident.
   */
  const blockForLine = useMemo(() => {
    const map = new Map<number, CfgBlock>();
    for (const b of cfg?.blocks ?? []) {
      if (!hasLines(b)) continue;
      for (let line = b.start_line; line <= b.end_line; line++) {
        const existing = map.get(line);
        const span = b.end_line - b.start_line;
        if (!existing || span < existing.end_line - existing.start_line)
          map.set(line, b);
      }
    }
    return map;
  }, [cfg]);

  /** Where the function sits in its file, so the reader opens on the subject rather than line 1. */
  const span = useMemo(() => cfgSpan(cfg), [cfg]);

  /** The blocks covering the selected block's lines, so the code pane can show what the graph means. */
  const highlighted = useMemo(() => {
    const b =
      selectedBlock === null ? null : (cfg?.blocks.find((x) => x.id === selectedBlock) ?? null);
    const lines = new Set<number>();
    if (b && hasLines(b))
      for (let line = b.start_line; line <= b.end_line; line++) lines.add(line);
    return { block: b, lines };
  }, [selectedBlock, cfg]);

  /**
   * The line to scroll to: the selected block's first.
   *
   * Clicking a rectangle already highlighted its lines; without the scroll it could highlight lines outside the
   * viewport, which makes the binding invisible in exactly the case it exists for.
   */
  const scrollToLine = useMemo(() => {
    const b = highlighted.block;
    return b && hasLines(b) ? b.start_line : null;
  }, [highlighted.block]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      {wantsGraph && (
        <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
          <Stat
            label="File"
            value={
              <span className="font-mono text-[12px]">
                {entry.file_path?.split("/").pop() ?? "—"}
              </span>
            }
            hint={
              entry.file_path ? relative(entry.file_path, prefix) : undefined
            }
          />
          <Stat
            label="Blocks"
            value={cfg?.blocks.length ?? "—"}
            hint={cfg ? `depth ${maxDominatorDepth(cfg)}` : undefined}
          />
          <Stat label="Control-flow edges" value={cfg?.edges?.length ?? "—"} />
          <Stat label="PDG nodes" value={entry.pdg_nodes ?? "—"} />
          <Stat
            label="Dominance frontier"
            value={cfg ? frontierBlockIds(cfg).size : "—"}
            hint="blocks where a φ belongs"
          />
          <Stat label="Exit blocks" value={cfg?.exits.length ?? "—"} />
        </div>
      )}

      <div className="flex min-h-0 flex-1 gap-3">
        {wantsSource && (
          <div className="flex min-w-0 flex-1 flex-col">
            <SourcePane
              entry={entry}
              // Only the graph-inclusive framings can bind a line to a block; with no graph beside it, a
              // clickable row would promise a selection nothing can show.
              blockForLine={wantsGraph ? blockForLine : undefined}
              selectedLines={wantsGraph ? highlighted.lines : undefined}
              onSelectBlock={
                wantsGraph ? (id) => setSelectedBlock(id === selectedBlock ? null : id) : undefined
              }
              focusRange={span}
              scrollToLine={wantsGraph ? scrollToLine : null}
              right={
                highlighted.block ? (
                  <Chip tone="blue">
                    {hasLines(highlighted.block) ? `B${highlighted.block.id} · lines ${highlighted.block.start_line}–${highlighted.block.end_line}` : `B${highlighted.block.id} · no source lines`}
                  </Chip>
                ) : span ? (
                  // No selection: say where the subject is, so the reader's arrival has a locator.
                  <Chip tone="blue">
                    lines {span.start}–{span.end}
                  </Chip>
                ) : undefined
              }
            />
          </div>
        )}

        {/* Equal width, not source-heavy: a CFG is wide and the source column is text that wraps, and at 3:2 the
            graph pane was too narrow to lay out even a modest function. */}
        {wantsGraph && (
          <div className="flex min-w-0 flex-1 flex-col">
            <Panel
              title="Control flow"
              subtitle={
                selectedBlock === null ? "hover a block for its dominator chain; click to select" : "selected — click again to clear"
              }
              right={
                <span className="flex items-center gap-1.5">
                  {headerRight}
                  <Toggle
                    on={showFrontier}
                    onClick={() => setShowFrontier((v) => !v)}
                  >
                    dominance frontier
                  </Toggle>
                  {cfg ? (
                    <Chip tone="green">{cfg.blocks.length} blocks</Chip>
                  ) : undefined}
                </span>
              }
            >
              {cfgState.kind === "loading" && (
                <p className="text-[11px] text-zinc-500">loading…</p>
              )}
              {/* An absence is stated here too: this pane previously said "loading…" forever for a subject the
                  bundle has no CFG for, which reads as a slow disk rather than a missing artifact. */}
              {(cfgState.kind === "absent" || cfgState.kind === "failed") && (
                <Unavailable
                  what={`No control-flow graph bundled for ${entry.name}`}
                  why={cfgState.reason}
                />
              )}
              {cfg && (
                <CfgGraph
                  cfg={cfg}
                  selected={selectedBlock}
                  onSelect={setSelectedBlock}
                  showFrontier={showFrontier}
                  onToggleFrontier={() => setShowFrontier((v) => !v)}
                  maxHeightClass={wantsSource ? "max-h-[52vh]" : "max-h-[70vh]"}
                  footnote={
                    <p className="text-[10px] leading-relaxed text-zinc-600">
                      Vertical position is dominator depth from{" "}
                      <code className="text-zinc-500">idom</code> — the layout
                      is the dominance relation, not a force simulation. Hover a
                      block to light up its dominator chain; click to read its
                      statements
                      {wantsSource ? ", and the source beside it follows the selection." : "."}{" "}
                      Amber dots are the blocks in a dominance frontier, where a
                      φ would be placed, and the toggle turns them off.
                    </p>
                  }
                />
              )}
            </Panel>

            {/* Below the graph, not above it: with the source showing, a block's statements belong beside the
                code they came from; without it, this is the only read-out. */}
            {highlighted.block && (
              <div className="mt-3">
                <BlockDetail
                  block={highlighted.block}
                  isExit={cfg?.exits.includes(highlighted.block.id) ?? false}
                  inFrontier={
                    cfg ? frontierBlockIds(cfg).has(highlighted.block.id) : false
                  }
                />
              </div>
            )}
            {footer}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * A labelled control in the graph header.
 *
 * The frontier overlay is a claim about the picture, so it is offered as a control that says whether it is on.
 */
function Toggle({
  on,
  onClick,
  children,
}: {
  on: boolean;
  onClick: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`rounded border px-1.5 py-0.5 text-[10px] ${
        on ? "border-amber-400/40 bg-amber-400/10 text-amber-200" : "border-white/10 text-zinc-500 hover:border-white/20 hover:text-zinc-300"
      }`}
    >
      {children}
    </button>
  );
}
