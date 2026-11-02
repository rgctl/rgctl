/**
 * The Spine lens — the focused symbol's source and its control-flow graph, one surface.
 *
 * This is the piece the maintainer named as missing on issue #61: "CFG, slice, blast, and dataflow only
 * make sense next to the actual source".
 *
 * ## One lens, not three
 *
 * This used to be three entries in the lens control: **Spine** (source beside the graph), **CFG·PDG** (the
 * same graph, `70vh`, plus a frontier toggle, exit count and dominator depth) and **Code** (the source
 * alone). They were not three ways of looking at a symbol — they were one surface with three framings, and
 * the drawing, the loader, the frontier set and the block panel were identical across them. Two of the
 * three were strictly poorer than this one, which is what made the control confusing: clicking a lens asked
 * "which frame?" instead of "which question?".
 *
 * So the framings became *controls* here. **Show source** collapses the source pane and gives the graph the
 * full width at `70vh` — what the CFG·PDG lens did — and the graph with the source collapsed is what the
 * Code lens was. What remains in the lens control is one implemented lens plus the genuinely different
 * questions phase 3 will fill (dataflow, slice, blast, metrics, time).
 *
 * ## The binding
 *
 * The graph and the code are two views of one Focus, and the binding runs **both ways**:
 *
 *   select a block  -> its source lines highlight, and the pane scrolls to them
 *   click a line    -> the block covering it is selected
 *
 * Both directions matter. One-way binding means the graph is authoritative and the code is a display;
 * two-way means either pane can be the one you are reading, which is how reviewing actually goes. With the
 * source hidden the selection still reads out as block statements below the graph, so nothing is lost.
 *
 * Scope discipline: no file tree, no editing, no go-to-definition. This is a view of the Focus, not an
 * IDE, and that boundary is what keeps the change reviewable.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import { relative, repoRoot, type CfgBlock, type FunctionEntry } from "../data";
import { useCfgDetail } from "../useCfgDetail";
import { BlockDetail } from "./BlockDetail";
import { CfgGraph, frontierBlockIds, hasLines, maxDominatorDepth } from "./CfgGraph";
import { SourcePane, cfgSpan } from "./SourcePane";
import { Chip, Command, Panel, Stat, Unavailable } from "../shell";

/**
 * A labelled control in the graph header.
 *
 * The frontier overlay is a claim about the picture, so it is offered as a control and says whether it is
 * on; `show source` is a reading preference and is offered the same way rather than as a second lens.
 */
function Toggle({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`rounded border px-1.5 py-0.5 text-[10px] ${
        on
          ? "border-amber-400/40 bg-amber-400/10 text-amber-200"
          : "border-white/10 text-zinc-500 hover:border-white/20 hover:text-zinc-300"
      }`}
    >
      {children}
    </button>
  );
}

export function Spine({ entry }: { entry: FunctionEntry }) {
  const cfgState = useCfgDetail(entry.function_id);
  const cfg = cfgState.kind === "ready" ? cfgState.cfg : null;
  const [selectedBlock, setSelectedBlock] = useState<number | null>(null);
  const [prefix, setPrefix] = useState("");
  /** Reading preferences, sticky across subjects: they describe how you read, not what this symbol is. */
  const [showSource, setShowSource] = useState(true);
  const [showFrontier, setShowFrontier] = useState(true);

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
   * When several blocks cover a line (nested or synthetic spans), the *smallest* span wins, because that
   * is the block the line most specifically belongs to. Blocks with no source lines are excluded outright
   * rather than matching line 0 by accident.
   */
  const blockForLine = useMemo(() => {
    const map = new Map<number, CfgBlock>();
    for (const b of cfg?.blocks ?? []) {
      if (!hasLines(b)) continue;
      for (let line = b.start_line; line <= b.end_line; line++) {
        const existing = map.get(line);
        const span = b.end_line - b.start_line;
        if (!existing || span < existing.end_line - existing.start_line) map.set(line, b);
      }
    }
    return map;
  }, [cfg]);

  /** Where the function sits in its file, so the reader opens on the subject rather than line 1. */
  const span = useMemo(() => cfgSpan(cfg), [cfg]);

  /** The blocks covering the selected block's lines, so the code pane can show what the graph means. */
  const highlighted = useMemo(() => {
    const b = selectedBlock === null ? null : (cfg?.blocks.find((x) => x.id === selectedBlock) ?? null);
    const lines = new Set<number>();
    if (b && hasLines(b)) for (let line = b.start_line; line <= b.end_line; line++) lines.add(line);
    return { block: b, lines };
  }, [selectedBlock, cfg]);

  /**
   * The line to scroll to: the selected block's first.
   *
   * Clicking a rectangle already highlighted its lines; without the scroll it could highlight lines outside
   * the viewport, which makes the binding invisible in exactly the case it exists for.
   */
  const scrollToLine = useMemo(() => {
    const b = highlighted.block;
    return b && hasLines(b) ? b.start_line : null;
  }, [highlighted.block]);

  const summary = useMemo(
    () => ({
      blocks: cfg?.blocks.length ?? null,
      edges: cfg?.edges?.length ?? 0,
      depth: cfg ? maxDominatorDepth(cfg) : null,
      // The same set the graph dots, so the count and the picture cannot disagree.
      frontiers: cfg ? frontierBlockIds(cfg).size : null,
      exits: cfg?.exits.length ?? null,
    }),
    [cfg],
  );

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 lg:grid-cols-6">
        <Stat
          label="File"
          value={<span className="font-mono text-[12px]">{entry.file_path?.split("/").pop() ?? "—"}</span>}
          hint={entry.file_path ? relative(entry.file_path, prefix) : undefined}
        />
        <Stat
          label="Blocks"
          value={summary.blocks ?? "—"}
          hint={summary.depth === null ? undefined : `depth ${summary.depth}`}
        />
        <Stat label="Control-flow edges" value={cfg ? summary.edges : "—"} />
        <Stat label="PDG nodes" value={entry.pdg_nodes ?? "—"} />
        <Stat label="Dominance frontier" value={summary.frontiers ?? "—"} hint="blocks where a φ belongs" />
        <Stat label="Exit blocks" value={summary.exits ?? "—"} />
      </div>

      <div className="flex min-h-0 flex-1 gap-3">
        {showSource && (
          <div className="flex min-w-0 flex-1 flex-col">
            <SourcePane
              entry={entry}
              blockForLine={blockForLine}
              selectedLines={highlighted.lines}
              onSelectBlock={(id) => setSelectedBlock(id === selectedBlock ? null : id)}
              focusRange={span}
              scrollToLine={scrollToLine}
              right={
                highlighted.block ? (
                  <Chip tone="blue">
                    {hasLines(highlighted.block)
                      ? `B${highlighted.block.id} · lines ${highlighted.block.start_line}–${highlighted.block.end_line}`
                      : `B${highlighted.block.id} · no source lines`}
                  </Chip>
                ) : span ? (
                  // No selection: say where the subject is, so the reader's arrival has a locator. This is
                  // the reading the retired Code lens offered above its source.
                  <Chip tone="blue">
                    lines {span.start}–{span.end}
                  </Chip>
                ) : undefined
              }
            />
          </div>
        )}

        {/* Equal width while the source shows, not source-heavy: a CFG is wide and the source column is text
            that wraps, and at 3:2 the graph pane was too narrow to lay out even a modest function. Collapsed,
            the graph is the whole surface at 70vh — the reading the retired CFG·PDG lens existed for. */}
        <div className="flex min-w-0 flex-1 flex-col">
          <Panel
            title="Control flow"
            subtitle={
              selectedBlock === null
                ? "hover a block for its dominator chain; click to select"
                : "selected — click again to clear"
            }
            right={
              <span className="flex items-center gap-1.5">
                <Toggle on={showFrontier} onClick={() => setShowFrontier((v) => !v)}>
                  dominance frontier
                </Toggle>
                <Toggle on={showSource} onClick={() => setShowSource((v) => !v)}>
                  show source
                </Toggle>
                {cfg ? <Chip tone="green">{cfg.blocks.length} blocks</Chip> : undefined}
              </span>
            }
          >
            {cfgState.kind === "loading" && <p className="text-[11px] text-zinc-500">loading…</p>}
            {/* An absence is stated here too: this pane previously said "loading…" forever for a subject the
                bundle has no CFG for, which reads as a slow disk rather than a missing artifact. */}
            {(cfgState.kind === "absent" || cfgState.kind === "failed") && (
              <Unavailable what={`No control-flow graph bundled for ${entry.name}`} why={cfgState.reason} />
            )}
            {cfg && (
              <CfgGraph
                cfg={cfg}
                selected={selectedBlock}
                onSelect={setSelectedBlock}
                showFrontier={showFrontier}
                onToggleFrontier={() => setShowFrontier((v) => !v)}
                maxHeightClass={showSource ? "max-h-[52vh]" : "max-h-[70vh]"}
                footnote={
                  <p className="text-[10px] leading-relaxed text-zinc-600">
                    Vertical position is dominator depth from <code className="text-zinc-500">idom</code> — the
                    layout is the dominance relation, not a force simulation. Hover a block to light up its
                    dominator chain; click to read its statements
                    {showSource ? ", and the source beside it follows the selection." : "."} Amber dots are the
                    blocks in a dominance frontier, where a φ would be placed, and the toggle turns them off.
                  </p>
                }
              />
            )}
          </Panel>

          {/* Below the graph, not above it: with the source showing, a block's statements belong beside the
              code they came from; with it hidden, this is the only read-out. */}
          {highlighted.block && (
            <div className="mt-3">
              <BlockDetail
                block={highlighted.block}
                isExit={cfg?.exits.includes(highlighted.block.id) ?? false}
                inFrontier={cfg ? frontierBlockIds(cfg).has(highlighted.block.id) : false}
              />
            </div>
          )}

          <Command>rgctl -r &lt;repo&gt; -f json inspect {entry.name} cfg</Command>
        </div>
      </div>
    </div>
  );
}
