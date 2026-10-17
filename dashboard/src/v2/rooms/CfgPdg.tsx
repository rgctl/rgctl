/**
 * The CFG·PDG lens — the control-flow graph as a surface of its own.
 *
 * The Spine already draws this graph, beside the source it belongs to. This lens exists for the case the
 * Spine cannot serve: when the graph *is* the thing being read. A 64-block function laid out by dominator
 * depth is unreadable in a half-pane, and the readings that matter here — how deep the dominance tree
 * goes, how many blocks sit on a dominance frontier, which blocks are exits — have nowhere to live beside
 * a source column.
 *
 * It is the same analysis, not a second one: the graph is `CfgGraph`, the walk is the exported
 * `dominatorDepths`, and the frontier set is the exported `frontierBlockIds`. What differs is the frame —
 * the stat row, the toggle and the height — which is the mock's `CfgLens`
 * (`docs/design/dashboard-v2/screens/02-focus-cfg-dominance.png`) brought forward onto live data.
 *
 * Honest about its own limits: the bundle stores at most a 6-statement preview per block and no CFG at all
 * for a function the analysis phase did not reach, so the block panel says the first and this lens states
 * the second rather than drawing an empty canvas.
 */
import { useEffect, useMemo, useState } from "react";
import type { CfgDetail, FunctionEntry } from "../data";
import { Chip, Command, Panel, Stat, Unavailable } from "../shell";
import { useCfgDetail } from "../useCfgDetail";
import { BlockDetail } from "./BlockDetail";
import { CfgGraph, frontierBlockIds, maxDominatorDepth } from "./CfgGraph";

const TITLE = "Control-flow graph";
const SUBTITLE = "compiler IR — dominators and dominance frontiers computed in-graph";

/**
 * The drawing, once the sidecar is known to be present.
 *
 * Split from the loader so the ready path holds no optional `cfg`: a graph component that has to keep
 * proving its data exists is where a stray non-null assertion hides a real absence.
 */
function CfgPdgReady({ entry, cfg }: { entry: FunctionEntry; cfg: CfgDetail }) {
  const [selected, setSelected] = useState<number | null>(null);
  /** Sticky across subjects on purpose: it is a reading preference, not part of the selection. */
  const [showFrontier, setShowFrontier] = useState(true);

  // A block id from the previous subject means nothing in this one, so the selection does not travel.
  useEffect(() => {
    setSelected(null);
  }, [entry.function_id]);

  const summary = useMemo(
    () => ({
      depth: maxDominatorDepth(cfg),
      edges: cfg.edges?.length ?? 0,
      // The same set the graph dots, so the count and the picture cannot disagree.
      frontiers: frontierBlockIds(cfg).size,
      exits: cfg.exits.length,
    }),
    [cfg],
  );

  const block = selected === null ? null : (cfg.blocks.find((b) => b.id === selected) ?? null);

  return (
    <div className="flex flex-col gap-2">
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-4">
        <Stat label="Blocks" value={cfg.blocks.length} hint={`depth ${summary.depth}`} />
        <Stat label="Control-flow edges" value={summary.edges} />
        <Stat label="Dominance frontiers" value={summary.frontiers} hint="blocks where a φ belongs" />
        <Stat label="Exit blocks" value={summary.exits} />
      </div>

      {/* Above the graph, not below it: the graph runs to 70vh, and a statement list underneath it is a
          list the reader has to scroll away from the block they just clicked. */}
      {block && (
        <BlockDetail
          block={block}
          isExit={cfg.exits.includes(block.id)}
          inFrontier={frontierBlockIds(cfg).has(block.id)}
        />
      )}

      <CfgGraph
        cfg={cfg}
        selected={selected}
        onSelect={setSelected}
        showFrontier={showFrontier}
        onToggleFrontier={() => setShowFrontier((v) => !v)}
        maxHeightClass="max-h-[70vh]"
        footnote={
          <p className="text-[10px] leading-relaxed text-zinc-600">
            Vertical position is dominator depth from <code className="text-zinc-500">idom</code> — the
            layout is the dominance relation, not a force simulation. Hover a block to light up its
            dominator chain; click to read its statements. Amber dots are the blocks in a dominance
            frontier, where a φ would be placed, and the toggle turns them off.
          </p>
        }
      />
    </div>
  );
}

export function CfgPdgLens({ entry }: { entry: FunctionEntry }) {
  const state = useCfgDetail(entry.function_id);

  const receipt = (
    <Command>
      rgctl -r &lt;repo&gt; -f json inspect {entry.name} cfg
    </Command>
  );

  if (state.kind === "loading") {
    return (
      <Panel title={TITLE} subtitle={SUBTITLE}>
        <p className="text-[11px] text-zinc-500">loading…</p>
      </Panel>
    );
  }

  if (state.kind === "absent" || state.kind === "failed") {
    return (
      <Panel title={TITLE} subtitle={SUBTITLE}>
        <Unavailable what={`No control-flow graph bundled for ${entry.name}`} why={state.reason} />
        {receipt}
      </Panel>
    );
  }

  const cfg = state.cfg;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <Panel
        title={TITLE}
        subtitle={SUBTITLE}
        right={
          <Chip tone="green">
            {cfg.blocks.length} blocks · {cfg.edges?.length ?? 0} edges
          </Chip>
        }
      >
        <CfgPdgReady entry={entry} cfg={cfg} />
      </Panel>

      {receipt}
    </div>
  );
}
