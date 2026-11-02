/**
 * The source pane — one reader, two lenses.
 *
 * **Spine** puts the control-flow graph beside this pane and binds the two directions: select a block and
 * its lines highlight, click a line and its block is selected. **Code** is this pane alone.
 *
 * The pane is shared because the Code lens always said it was: its `App.tsx` body read "the Code lens is
 * the same source pane as Spine without the control-flow graph beside it. Source rendering is shared, so
 * this lens is a view of the same reader rather than a second implementation" — while rendering **no
 * source at all** and no typed absence. A sentence describing a shared reader, with only one reader and no
 * second caller, is a claim about code that does not exist. This file is that reader; both lenses are now
 * its callers, and the sentence is true.
 *
 * What stays out, deliberately, in both callers: no file tree, no editing, no go-to-definition. This is a
 * view of the Focus, not an IDE.
 */
import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  excerpt,
  loadSourceForPath,
  relative,
  repoRoot,
  type CfgBlock,
  type FunctionEntry,
} from "../data";
import { highlightLine, TOKEN_CLASS } from "../highlight";
import { Panel, Unavailable } from "../shell";
import { hasLines, lineLabel } from "./CfgGraph";

type SourceState =
  | { kind: "loading" }
  | { kind: "ready"; lines: { line: number; text: string }[] }
  | { kind: "absent"; reason: string };

/** Language id for highlighting, from the file extension. */
export function languageFor(filePath: string | null | undefined): string {
  const ext = filePath?.split(".").pop()?.toLowerCase() ?? "";
  const map: Record<string, string> = {
    ts: "typescript", tsx: "typescript", mts: "typescript", cts: "typescript",
    js: "javascript", jsx: "javascript", mjs: "javascript", cjs: "javascript",
    rs: "rust", py: "python", go: "go", java: "java", cs: "csharp",
    cpp: "cpp", cc: "cpp", hpp: "cpp", c: "c", h: "c", php: "php", rb: "ruby",
  };
  return map[ext] ?? "typescript";
}

export interface SourcePaneProps {
  entry: FunctionEntry;
  /**
   * Line → owning block, from the caller's CFG. Omitted by Code, which has no graph beside it — so no row
   * is clickable and no row claims a block it cannot show.
   */
  blockForLine?: Map<number, CfgBlock>;
  /** Lines to highlight, from the caller's selected block. */
  selectedLines?: Set<number>;
  /** Omitted by Code for the same reason as `blockForLine`. */
  onSelectBlock?: (blockId: number | null) => void;
  /** Right-hand slot in the panel header (Spine puts the selected-block chip here). */
  right?: ReactNode;
}

export function SourcePane({ entry, blockForLine, selectedLines, onSelectBlock, right }: SourcePaneProps) {
  const [source, setSource] = useState<SourceState>({ kind: "loading" });
  const [prefix, setPrefix] = useState("");

  useEffect(() => {
    let cancelled = false;
    setSource({ kind: "loading" });

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

  const language = useMemo(() => languageFor(entry.file_path), [entry.file_path]);
  const highlighted = selectedLines ?? EMPTY;

  return (
    <Panel title="Source" subtitle={entry.file_path ? relative(entry.file_path, prefix) : undefined} right={right}>
      {source.kind === "loading" && <p className="text-[11px] text-zinc-500">loading…</p>}
      {source.kind === "absent" && <Unavailable what="Source text is not available" why={source.reason} />}
      {source.kind === "ready" && (
        <div className="max-h-[56vh] overflow-auto">
          <table className="w-full border-collapse font-mono text-[11px] leading-[1.55]">
            <tbody>
              {source.lines.map(({ line, text }) => {
                const owner = blockForLine?.get(line);
                const inSelection = highlighted.has(line);
                return (
                  <tr
                    key={line}
                    onClick={() => owner && onSelectBlock?.(owner.id)}
                    title={owner ? `block B${owner.id} (${lineLabel(owner)})` : undefined}
                    className={
                      inSelection
                        ? "bg-sky-500/15"
                        : owner && onSelectBlock
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
  );
}

const EMPTY: Set<number> = new Set();
