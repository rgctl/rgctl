/**
 * The Spine lens — source for the focused symbol, beside its control-flow graph.
 *
 * This is the piece the maintainer named as missing on issue #61: "CFG, slice, blast, and dataflow only
 * make sense next to the actual source". The graph and the code are two views of one Focus, and the
 * binding runs **both ways**:
 *
 *   select a block  -> its source lines highlight
 *   click a line    -> the block covering it is selected
 *
 * Both directions matter. One-way binding means the graph is authoritative and the code is a display;
 * two-way means either pane can be the one you are reading, which is how reviewing actually goes.
 *
 * The source pane itself lives in `SourcePane.tsx`, shared with the Code lens — this file owns the graph,
 * the two-way binding and the block read-out.
 *
 * Scope discipline: no file tree, no editing, no go-to-definition. This is a view of the Focus, not an
 * IDE, and that boundary is what keeps the change reviewable.
 */
import { useEffect, useMemo, useState } from "react";
import { relative, repoRoot, type CfgBlock, type FunctionEntry } from "../data";
import { useCfgDetail } from "../useCfgDetail";
import { BlockDetail } from "./BlockDetail";
import { CfgGraph, frontierBlockIds, hasLines } from "./CfgGraph";
import { SourcePane, cfgSpan } from "./SourcePane";
import { Chip, Command, Panel, Stat, Unavailable } from "../shell";

export function Spine({ entry }: { entry: FunctionEntry }) {
  const cfgState = useCfgDetail(entry.function_id);
  const cfg = cfgState.kind === "ready" ? cfgState.cfg : null;
  const [selectedBlock, setSelectedBlock] = useState<number | null>(null);
  const [prefix, setPrefix] = useState("");

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

  /**
   * The line to scroll to: the selected block's first, or the function's first when nothing is selected.
   *
   * Clicking a rectangle already highlighted its lines; before this it could highlight lines outside the
   * scrolled viewport, which makes the binding invisible in exactly the case it exists for.
   */
  const scrollToLine = useMemo(() => {
    const b = selectedBlock === null ? null : (cfg?.blocks.find((x) => x.id === selectedBlock) ?? null);
    if (b && hasLines(b)) return b.start_line;
    return null;
  }, [selectedBlock, cfg]);

  /** The blocks covering the selected block's lines, so the code pane can show what the graph means. */
  const highlighted = useMemo(() => {
    const b = selectedBlock === null ? null : (cfg?.blocks.find((x) => x.id === selectedBlock) ?? null);
    const lines = new Set<number>();
    if (b && hasLines(b)) for (let line = b.start_line; line <= b.end_line; line++) lines.add(line);
    return { block: b, lines };
  }, [selectedBlock, cfg]);

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat
          label="File"
          value={<span className="font-mono text-[12px]">{entry.file_path?.split("/").pop() ?? "—"}</span>}
          hint={entry.file_path ? relative(entry.file_path, prefix) : undefined}
        />
        <Stat label="Blocks" value={cfg?.blocks.length ?? "—"} hint={cfg ? `${cfg.edges?.length ?? 0} edges` : undefined} />
        <Stat label="PDG nodes" value={entry.pdg_nodes ?? "—"} />
        <Stat
          label="Dominance frontier"
          value={cfg ? frontierBlockIds(cfg).size : "—"}
          hint="blocks where a φ belongs"
        />
      </div>

      <div className="flex min-h-0 flex-1 gap-3">
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
              ) : undefined
            }
          />
        </div>

        {/* Equal width, not source-heavy: a CFG is wide and the source column is text that wraps. At 3:2 the
            graph pane was too narrow to lay out even a modest function, which made the graph the pane you
            scrolled rather than the pane you read. */}
        <div className="flex min-w-0 flex-1 flex-col">
          <Panel
            title="Control flow"
            subtitle={selectedBlock === null ? "hover a block for its dominator chain; click to select" : "selected — click again to clear"}
            right={cfg ? <Chip tone="green">{cfg.blocks.length} blocks</Chip> : undefined}
          >
            {cfgState.kind === "loading" && <p className="text-[11px] text-zinc-500">loading…</p>}
            {/* An absence is stated here too: this pane previously said "loading…" forever for a subject
                the bundle has no CFG for, which reads as a slow disk rather than a missing artifact. */}
            {(cfgState.kind === "absent" || cfgState.kind === "failed") && (
              <Unavailable what={`No control-flow graph bundled for ${entry.name}`} why={cfgState.reason} />
            )}
            {cfg && <CfgGraph cfg={cfg} selected={selectedBlock} onSelect={setSelectedBlock} />}
          </Panel>

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
