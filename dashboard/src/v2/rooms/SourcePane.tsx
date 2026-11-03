/**
 * The source pane — one reader, two lenses.
 *
 * The **CFG** lens puts the control-flow graph beside this pane and binds the two directions: select a block
 * and its lines highlight, click a line and its block is selected. With `show source` off, this pane is
 * collapsed and the graph takes the whole surface.
 *
 * The pane was extracted because the retired Code lens always said it was shared: its body read "the Code lens is
 * the same source pane as Spine without the control-flow graph beside it. Source rendering is shared, so
 * this lens is a view of the same reader rather than a second implementation" — while rendering **no
 * source at all** and no typed absence. A sentence describing a shared reader, with only one reader and no
 * second caller, is a claim about code that does not exist. This file is that reader; both lenses are now
 * its callers, and the sentence is true.
 *
 * What stays out, deliberately, in both callers: no file tree, no editing, no go-to-definition. This is a
 * view of the Focus, not an IDE.
 */
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import {
  excerpt,
  loadSourceForPath,
  relative,
  repoRoot,
  type CfgBlock,
  type CfgDetail,
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
  /** Right-hand slot in the panel header (the CFG lens puts the selected-block chip here). */
  right?: ReactNode;
  /**
   * The function's own lines, when the caller can name them from its CFG.
   *
   * Both lenses pass this, and it changes what the pane does on arrival: the file used to open at line 1
   * with nothing marked, so a 746-line file looked like a 746-line file and the subject was somewhere in
   * the middle of it. The reader now scrolls to the first line of the subject and keeps the whole span
   * faintly marked, so "where am I" has an answer before any click.
   */
  focusRange?: { start: number; end: number } | null;
  /**
   * Scroll this line into view when it changes.
   *
   * The CFG lens passes the start line of the block you just clicked: selecting a rectangle already highlighted
   * its lines, but without this the highlight could sit outside the scrolled viewport — a graph click that
   * changes something you cannot see is half a binding.
   */
  scrollToLine?: number | null;
}

/** The span a function occupies in its file, from its CFG blocks. `null` when there is no CFG to ask. */
export function cfgSpan(cfg: CfgDetail | null): { start: number; end: number } | null {
  const blocks = (cfg?.blocks ?? []).filter(hasLines);
  if (blocks.length === 0) return null;
  let start = Infinity;
  let end = -Infinity;
  for (const b of blocks) {
    if (b.start_line < start) start = b.start_line;
    if (b.end_line > end) end = b.end_line;
  }
  return { start, end };
}

/**
 * Where the reader was standing in each file, keyed by function.
 *
 * Module scope, not component state: switching lens unmounts the pane, so component state would be gone by
 * the time you came back. Keyed by function rather than by lens because the file is the thing you were
 * reading — jumping back to the function's first line on every lens switch throws away the one piece of
 * state the reader built by hand.
 */
const readingPosition = new Map<string, number>();

function reducedMotion(): boolean {
  return (
    typeof window !== "undefined" &&
    typeof window.matchMedia === "function" &&
    window.matchMedia("(prefers-reduced-motion: reduce)").matches
  );
}

/**
 * Centre a line inside the pane's own scroller.
 *
 * Deliberately **not** `Element.scrollIntoView`: that scrolls every scrollable ancestor, so centring a row
 * also moved the room's outer `overflow-y-auto` container and the page appeared to jump on every lens
 * switch. This touches only `scroller.scrollTop`, and computes the offset from bounding boxes because
 * `offsetTop` of a `<tr>` is relative to an ancestor that differs by layout.
 */
function centerLine(scroller: HTMLElement | null, line: number | null, smooth: boolean) {
  if (line === null || scroller === null) return;
  const row = scroller.querySelector<HTMLElement>(`tr[data-line="${line}"]`);
  if (!row) return;
  const frame = scroller.getBoundingClientRect();
  const target = row.getBoundingClientRect();
  const delta = target.top - frame.top - frame.height / 2 + target.height / 2;
  const top = Math.max(0, scroller.scrollTop + delta);
  if (smooth && typeof scroller.scrollTo === "function") {
    scroller.scrollTo({ top, behavior: "smooth" });
  } else {
    scroller.scrollTop = top;
  }
}

export function SourcePane({
  entry,
  blockForLine,
  selectedLines,
  onSelectBlock,
  right,
  focusRange,
  scrollToLine,
}: SourcePaneProps) {
  const [source, setSource] = useState<SourceState>({ kind: "loading" });
  const [prefix, setPrefix] = useState("");
  const scroller = useRef<HTMLDivElement>(null);

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
  const ready = source.kind === "ready";
  const rangeStart = focusRange?.start ?? null;
  const rangeEnd = focusRange?.end ?? null;

  /**
   * On arrival: resume where this file was left, and only fall back to the subject's first line.
   *
   * Restoring before scrolling is the whole point — a lens switch remounts the pane, and re-centring the
   * function on every switch discards the position the reader scrolled to by hand. Reading is not a
   * sequence of arrivals.
   */
  useEffect(() => {
    if (!ready) return;
    const saved = readingPosition.get(entry.function_id);
    if (saved !== undefined) {
      if (scroller.current) scroller.current.scrollTop = saved;
      return;
    }
    centerLine(scroller.current, rangeStart, false);
  }, [ready, rangeStart, entry.function_id]);

  // A block the caller selected moves the reader to that block. Keyed on the line, not on the call, so
  // re-selecting the same block does not fight a manual scroll.
  useEffect(() => {
    if (!ready) return;
    centerLine(scroller.current, scrollToLine ?? null, !reducedMotion());
  }, [ready, scrollToLine, entry.function_id]);

  return (
    <Panel title="Source" subtitle={entry.file_path ? relative(entry.file_path, prefix) : undefined} right={right}>
      {source.kind === "loading" && <p className="text-[11px] text-zinc-500">loading…</p>}
      {source.kind === "absent" && <Unavailable what="Source text is not available" why={source.reason} />}
      {source.kind === "ready" && (
        <div
          ref={scroller}
          data-testid="source-scroller"
          onScroll={(e) => readingPosition.set(entry.function_id, e.currentTarget.scrollTop)}
          className="max-h-[56vh] overflow-auto"
        >
          <table className="w-full border-collapse font-mono text-[11px] leading-[1.55]">
            <tbody>
              {source.lines.map(({ line, text }) => {
                const owner = blockForLine?.get(line);
                const inSelection = highlighted.has(line);
                const inRange = rangeStart !== null && rangeEnd !== null && line >= rangeStart && line <= rangeEnd;
                return (
                  <tr
                    key={line}
                    data-line={line}
                    onClick={() => owner && onSelectBlock?.(owner.id)}
                    title={owner ? `block B${owner.id} (${lineLabel(owner)})` : undefined}
                    className={
                      inSelection
                        ? "bg-sky-500/15"
                        : inRange
                          ? "bg-white/[0.035]"
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
