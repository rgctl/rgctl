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
 * Scope discipline: no file tree, no editing, no go-to-definition. This is a view of the Focus, not an
 * IDE, and that boundary is what keeps the change reviewable.
 */
import { useEffect, useMemo, useState } from "react";
import {
  excerpt,
  loadSourceForPath,
  relative,
  repoRoot,
  type CfgBlock,
  type FunctionEntry,
} from "../data";
import { useCfgDetail } from "../useCfgDetail";
import { BlockDetail } from "./BlockDetail";
import { CfgGraph, frontierBlockIds, hasLines, lineLabel } from "./CfgGraph";
import { highlightLine, TOKEN_CLASS } from "../highlight";
import { Chip, Command, Panel, Stat, Unavailable } from "../shell";

type SourceState =
  | { kind: "loading" }
  | { kind: "ready"; lines: { line: number; text: string }[] }
  | { kind: "absent"; reason: string };

/** Language id for highlighting, from the file extension. */
function languageFor(filePath: string | null | undefined): string {
  const ext = filePath?.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
    js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
    rs: "rust", py: "python", go: "go", java: "java", cs: "csharp",
    cpp: "cpp", cc: "cpp", hpp: "cpp", c: "c", h: "c", php: "php", rb: "ruby",
  };
  return map[ext] ?? "typescript";
}

export function Spine({ entry }: { entry: FunctionEntry }) {
  const cfgState = useCfgDetail(entry.function_id);
  const cfg = cfgState.kind === "ready" ? cfgState.cfg : null;
  const [source, setSource] = useState<SourceState>({ kind: "loading" });
  const [selectedBlock, setSelectedBlock] = useState<number | null>(null);
  const [prefix, setPrefix] = useState("");

  useEffect(() => {
    let cancelled = false;
    setSource({ kind: "loading" });
    setSelectedBlock(null);

    void repoRoot().then((p) => {
      if (!cancelled) setPrefix(p);
    });

    void (async () => {
      if (!entry.file_path) {
        setSource({ kind: "absent", reason: "This index entry carries no file path, so there is nothing to read." });
        return;
      }
      const text = await loadSourceForPath(entry.file_path);
      if (cancelled) return;
      if (text === null) {
        setSource({
          kind: "absent",
          reason:
            "This bundle did not export source text. `sources/` is written by the CFG/slice export, so a bundle discovered without it has no source to show.",
        });
        return;
      }
      setSource({ kind: "ready", lines: excerpt(text, 1, text.split("\n").length) });
    })();

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

  /** The blocks covering the selected block's lines, so the code pane can show what the graph means. */
  const highlighted = useMemo(() => {
    const b = selectedBlock === null ? null : (cfg?.blocks.find((x) => x.id === selectedBlock) ?? null);
    const lines = new Set<number>();
    if (b && hasLines(b)) for (let line = b.start_line; line <= b.end_line; line++) lines.add(line);
    return { block: b, lines };
  }, [selectedBlock, cfg]);

  const language = languageFor(entry.file_path);

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
          <Panel
            title="Source"
            subtitle={entry.file_path ? relative(entry.file_path, prefix) : undefined}
            right={
              highlighted.block ? (
                <Chip tone="blue">
                  {hasLines(highlighted.block)
                    ? `B${highlighted.block.id} · lines ${highlighted.block.start_line}–${highlighted.block.end_line}`
                    : `B${highlighted.block.id} · no source lines`}
                </Chip>
              ) : undefined
            }
          >
            {source.kind === "loading" && <p className="text-[11px] text-zinc-500">loading…</p>}
            {source.kind === "absent" && <Unavailable what="Source text is not available" why={source.reason} />}
            {source.kind === "ready" && (
              <div className="max-h-[56vh] overflow-auto">
                <table className="w-full border-collapse font-mono text-[11px] leading-[1.55]">
                  <tbody>
                    {source.lines.map(({ line, text }) => {
                      const owner = blockForLine.get(line);
                      const inSelection = highlighted.lines.has(line);
                      return (
                        <tr
                          key={line}
                          onClick={() => owner && setSelectedBlock(owner.id === selectedBlock ? null : owner.id)}
                          title={owner ? `block B${owner.id} (${lineLabel(owner)})` : undefined}
                          className={
                            inSelection
                              ? "bg-sky-500/15"
                              : owner
                                ? "cursor-pointer hover:bg-white/[0.04]"
                                : undefined
                          }
                        >
                          <td className="w-14 select-none pr-3 text-right align-top text-zinc-600">
                            {line}
                            {inSelection && <span className="ml-1 text-sky-400">▍</span>}
                          </td>
                          <td className="whitespace-pre-wrap break-all align-top">
                            {highlightLine(text, language).map((tok, i) => (
                              <span key={i} className={TOKEN_CLASS[tok.kind]}>
                                {tok.text}
                              </span>
                            ))}
                            {text === "" && "\u00a0"}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </Panel>
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
              <Unavailable
                what={`No control-flow graph bundled for ${entry.name}`}
                why={cfgState.reason}
              />
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
