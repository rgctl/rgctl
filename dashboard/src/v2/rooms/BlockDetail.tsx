/**
 * One block of a CFG, read out.
 *
 * Shared by the Spine lens, where it sits under the source pane, and the CFG·PDG lens, where it sits above
 * the graph. Two placements, one implementation: the alternative is two statement renderers that drift, and
 * the first thing to drift would be the honesty rule below.
 *
 * The honesty rule: the export keeps at most a **6-statement preview** per block
 * (`crates/rgctl-dashboard/src/cfg_export.rs`, `statements.iter().take(6)`), so a dense block is truncated
 * in the bundle. A panel that prints "6 statements" for such a block is reporting its own limit as the
 * program's, which is why the count here is labelled *shown* and the cap is stated when it is hit.
 */
import type { CfgBlock } from "../data";
import { Chip } from "../shell";
import { hasLines, lineLabel } from "./CfgGraph";

/** The per-block statement preview cap in the bundle. Kept in sync with `cfg_export.rs`. */
export const STATEMENT_PREVIEW_CAP = 6;

export function BlockDetail({
  block,
  isExit = false,
  inFrontier = false,
}: {
  block: CfgBlock;
  isExit?: boolean;
  inFrontier?: boolean;
}) {
  const shown = block.statements.length;
  const capped = shown >= STATEMENT_PREVIEW_CAP;

  return (
    <div className="rounded-lg border border-white/8 bg-white/[0.02] p-3">
      <div className="flex flex-wrap items-center gap-2">
        <Chip tone="blue">B{block.id}</Chip>
        <span className="text-[11px] text-zinc-300">{lineLabel(block)}</span>
        <span className="text-[10px] text-zinc-500">
          {shown} statement(s) shown{capped ? " — at the bundle's 6-statement preview cap" : ""}
        </span>
        {isExit && <Chip tone="blue">exit</Chip>}
        {inFrontier && <Chip tone="amber">in a dominance frontier</Chip>}
      </div>
      <div className="mt-2 space-y-0.5">
        {shown === 0 && (
          <span className="text-[11px] text-zinc-600">
            no statements recorded for this block
            {!hasLines(block) && " — it covers no source lines (a synthetic prologue or epilogue block)"}
          </span>
        )}
        {block.statements.map((s, i) => (
          <pre
            key={i}
            className="overflow-x-auto rounded border border-white/8 bg-black/40 px-2 py-1 font-mono text-[11px] text-emerald-200/90"
          >
            {s}
          </pre>
        ))}
      </div>
    </div>
  );
}
