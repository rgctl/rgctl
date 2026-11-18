/**
 * Understand room — the Focus's source, CFG, PDG, and blast lenses.
 *
 * This module is deliberately its own lazy chunk. The shell must not parse CFG/PDG/Blast code just to
 * render Architecture; `/understand?lens=cfg` loads this room, not the whole application.
 */
import { useState } from "react";
import type { FunctionEntry } from "../data";
import type { LensId, RoomId } from "../focus";
import { useBlastRadius } from "../hooks/useBlastRadius";
import { useDfgDetail } from "../hooks/useDfgDetail";
import { Chip, Command, Panel, Unavailable } from "../shell";
import { BlastStage } from "./BlastStage";
import { Cfg } from "./Cfg";
import { DfgGraph } from "./DfgGraph";
import { LensPlaceholder } from "./Placeholder";

/**
 * PDG / dataflow lens — the Taint room's DFG canvas, attached to the Focus.
 *
 * `useDfgDetail` owns the archive/per-file decision and the four-valued absence state; this component
 * only chooses the surface and the receipt. The graph component is shared so the Taint and Understand
 * rooms cannot drift into drawing two different PDGs.
 */
function DataflowLens({ entry }: { entry: FunctionEntry }) {
  const dfg = useDfgDetail(entry.function_id, true, "slice");
  return (
    <Panel
      title="PDG / Dataflow"
      subtitle="def-use edges for this symbol"
      right={dfg.kind === "ready" ? <Chip tone="blue">PDG</Chip> : undefined}
    >
      {(dfg.kind === "idle" || dfg.kind === "loading") && (
        <p className="text-[11px] text-zinc-500">Loading PDG…</p>
      )}
      {(dfg.kind === "absent" || dfg.kind === "failed") && (
        <Unavailable
          tag={dfg.kind === "failed" ? "read failed" : "no PDG"}
          what={
            dfg.kind === "failed"
              ? "Could not read the PDG for this subject"
              : "No PDG / dataflow detail for this subject"
          }
          why={dfg.reason}
        />
      )}
      {dfg.kind === "ready" && (
        <DfgGraph dfg={dfg.dfg} maxHeightClass="max-h-[58vh]" testId="understand-dfg-stage" />
      )}
      <Command>rgctl -r &lt;repo&gt; -f json cpg pdg {entry.name}</Command>
    </Panel>
  );
}

/**
 * Blast-radius lens — the Taint room's blast canvas, attached to the Focus.
 *
 * Depth is the only control: it is the search radius, and the worker debounces it. The engine is lazy,
 * so opening Understand never downloads graph_payload until this lens is selected.
 */
function BlastLens({ entry }: { entry: FunctionEntry }) {
  const [depth, setDepth] = useState(5);
  const blast = useBlastRadius(entry.function_id, depth, true);
  return (
    <Panel
      title="Blast radius"
      subtitle="upstream callers by hop depth"
      right={blast.kind === "ready" ? <Chip tone="amber">live</Chip> : undefined}
    >
      <BlastStage
        seedName={entry.name}
        depth={depth}
        onDepthChange={setDepth}
        score={null}
        blast={blast}
        testIdPrefix="understand"
      />
      <Command>
        rgctl -r &lt;repo&gt; -f json blast-radius {entry.name} --depth {depth}
      </Command>
    </Panel>
  );
}

export function UnderstandRoom({
  entry,
  lens,
  loading,
  onOpenPalette,
  onCarry,
}: {
  entry: FunctionEntry | null;
  lens: LensId;
  loading: boolean;
  onOpenPalette: () => void;
  onCarry: (r: RoomId) => void;
}) {
  if (!entry) {
    return (
      <Panel title="Understand" subtitle="no subject selected">
        <p className="text-[12px] text-zinc-400">
          {loading ? (
            "Loading the function index…"
          ) : (
            <>
              Press <span className="rounded border border-white/12 px-1 font-mono text-[11px]">⌘K</span> or pick a
              symbol to give this room a subject. A room never demands a selection before rendering — it falls
              back to a useful default rather than an empty state.
            </>
          )}
        </p>
        <button
          type="button"
          onClick={onOpenPalette}
          className="mt-3 rounded border border-white/12 bg-white/[0.03] px-2.5 py-1 text-[11px] text-zinc-300 hover:border-sky-400/40"
        >
          Search symbols
        </button>
      </Panel>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <Panel title={entry.name} subtitle={entry.file_path ?? undefined} right={<Chip tone="blue">focus</Chip>}>
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">carry this focus to</span>
          {(["review", "taint", "architecture", "migration", "proof"] as RoomId[]).map((r) => (
            <button
              key={r}
              type="button"
              onClick={() => onCarry(r)}
              className="rounded border border-white/12 bg-white/[0.03] px-2 py-0.5 text-[11px] text-zinc-300 hover:border-sky-400/40 hover:text-sky-200"
            >
              {r} →
            </button>
          ))}
        </div>
      </Panel>

      {lens === "cfg" && <Cfg entry={entry} />}
      {lens === "dataflow" && <DataflowLens entry={entry} />}
      {lens === "blast" && <BlastLens entry={entry} />}
      {/* Slice/Trace, Metrics, and Time remain questions phase 3 answers. `code` and `cfgpdg` are gone:
          both were the CFG lens in another frame, and both are now controls inside it. */}
      {lens !== "cfg" && lens !== "dataflow" && lens !== "blast" && (
        <LensPlaceholder lens={lens} />
      )}
    </div>
  );
}
