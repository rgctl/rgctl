/**
 * The **CFG** lens — the focused symbol's source and its control-flow graph, one surface.
 *
 * Named CFG, not "Spine": the lens draws the control-flow graph, and that is the term a reader already has.
 * "Spine" was our word for the composition of source + graph, and a word nobody knows is a word nobody
 * clicks.
 *
 * This is the piece the maintainer named as missing on issue #61: "CFG, slice, blast, and dataflow only
 * make sense next to the actual source".
 *
 * ## One lens, not three
 *
 * This used to be three entries in the lens control: **Spine** (source beside the graph), **CFG·PDG** (the
 * same graph, `70vh`, plus a frontier toggle, exit count and dominator depth) and **Code** (the source
 * alone) — the first of the three is the one that survives, under the name CFG. They were not three ways of looking at a symbol — they were one surface with three framings, and
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
import { useState, type ReactNode } from "react";
import { Command } from "../shell";
import type { FunctionEntry } from "../data";
import { SymbolReader } from "./SymbolReader";

/**
 * A labelled control above the reading.
 *
 * `show source` is a reading preference, not a lens: it says how you read, so it sticks across subjects.
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

export function Cfg({ entry }: { entry: FunctionEntry }) {
  const [showSource, setShowSource] = useState(true);

  return (
    <SymbolReader
      entry={entry}
      variant={showSource ? "source-and-graph" : "graph"}
      headerRight={
        <Toggle on={showSource} onClick={() => setShowSource((v) => !v)}>
          show source
        </Toggle>
      }
      footer={<Command>rgctl -r &lt;repo&gt; -f json inspect {entry.name} cfg</Command>}
    />
  );
}
