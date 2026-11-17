import { lazy, Suspense, useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Instrument } from "./Instrument";
import { Palette } from "./Palette";
import { Chip, Command, Panel, Unavailable } from "./shell";
import { Cfg } from "./rooms/Cfg";
import { LensPlaceholder, Placeholder } from "./rooms/Placeholder";

import { DfgGraph } from "./rooms/DfgGraph";
import { BlastStage } from "./rooms/BlastStage";
import { useDfgDetail } from "./hooks/useDfgDetail";
import { useBlastRadius } from "./hooks/useBlastRadius";
import { loadManifest, type FunctionEntry } from "./data";
import { LENSES, ROOMS, keyOf, type LensId, type RoomId } from "./focus";
import { useFocus } from "./useFocus";

// Heavy rooms are their own chunks. The shell renders immediately; a direct
// `/understand` link must not parse Architecture's Three.js room just to show a CFG.
const ArchitectureRoom = lazy(() =>
  import("./rooms/Architecture").then((m) => ({ default: m.ArchitectureRoom })),
);
const MigrationRoom = lazy(() =>
  import("./rooms/Migration").then((m) => ({ default: m.MigrationRoom })),
);
const TaintRoom = lazy(() =>
  import("./rooms/Taint").then((m) => ({ default: m.TaintRoom })),
);

export function App() {
  // Declared before `useFocus` because the function index is view-scoped: Architecture
  // does not need the 67 MB cfg/slice indexes until the palette opens.
  const [paletteOpen, setPaletteOpen] = useState(false);
  const {
    room,
    lens,
    entry,
    staleFocus,
    functions: list,
    functionsLoading,
    functionsError,
    explode,
    setFocus,
    setLens,
    setExplode,
    goRoom,
    pick: focusPick,
    pickKey: focusPickKey,
  } = useFocus({
    enableFunctions: (room, focus) =>
      paletteOpen || focus !== null || room !== "architecture",
  });

  const manifest = useQuery({ queryKey: ["manifest"], queryFn: loadManifest });

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOpen((v) => !v);
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  /**
   * Picking a symbol sets the Focus (key only). From Repo, land in Understand so the graph is one
   * action away; other rooms keep their place.
   */
  const pick = (f: FunctionEntry) => {
    setPaletteOpen(false);
    focusPick(f);
  };

  const pickKey = (key: string) => {
    setPaletteOpen(false);
    focusPickKey(key);
  };

  return (
    <div className="flex h-full flex-col overflow-hidden">
      <Instrument
        manifest={manifest.data ?? null}
        focusLabel={entry?.name ?? null}
        onClearFocus={() => setFocus(null)}
      />

      <div className="px-3 pb-2 pt-2">
        <button
          type="button"
          onClick={() => setPaletteOpen(true)}
          className="flex w-full items-center gap-2 rounded border border-white/10 bg-white/[0.03] px-2.5 py-1.5 text-left text-[12px] text-zinc-500 hover:border-white/20 hover:text-zinc-300"
        >
          <span>⌕</span>
          <span>Search symbols, communities, or jump to a room…</span>
          <span className="ml-auto rounded border border-white/10 px-1.5 text-[10px]">⌘K</span>
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col">
        {/*
          One horizontal row of rooms, not a 208px column. The rail cost the full width of the left edge in every
          room, and the room that needs width most — the universe — paid it twice: 208px of sidebar plus a 300px
          column. Room labels wrap on one line; the question a room answers stays visible for the room you are
          in, which is the only one where it is still a question.
        */}
        <div className="flex shrink-0 flex-col gap-1 border-b border-white/8 bg-[#0e1116] px-3 py-1.5">
          <div className="flex flex-wrap items-center gap-1">
            <span className="mr-1 text-[10px] uppercase tracking-wider text-zinc-600">rooms</span>
            <nav aria-label="Rooms" className="flex flex-wrap items-center gap-1">
              {ROOMS.map((r) => (
                <button
                  key={r.id}
                  type="button"
                  onClick={() => goRoom(r.id)}
                  aria-current={room === r.id ? "page" : undefined}
                  title={r.blurb}
                  // Rooms are the one control that is always on screen, so they carry a surface rather than
                  // being text that happens to be clickable: a deep teal chip at rest, a lighter one with a
                  // sky edge for the room you are in.
                  className={`rounded-md border px-3 py-1.5 text-[12px] leading-none transition-colors ${
                    room === r.id
                      ? "border-sky-400/70 bg-[#16566e] font-medium text-white"
                      : "border-[#1d4b5e] bg-[#0d2f3e] text-zinc-200 hover:border-[#2b6d88] hover:bg-[#12455a] hover:text-white"
                  }`}
                >
                  {r.label}
                </button>
              ))}
            </nav>
            <span className="ml-auto flex items-center gap-2 text-[10px] text-zinc-600">
              <span className="hidden sm:inline">{ROOMS.find((r) => r.id === room)?.blurb}</span>
              {entry && <Chip tone="blue">subject: {entry.name}</Chip>}
            </span>
          </div>
          {/* Lenses are Understand controls — hiding them in other rooms avoids implying CFG/Dataflow
              tabs apply to Migration/Taint/Architecture. */}
          {room === "understand" && (
            <div className="flex flex-wrap items-center gap-1">
              <span className="mr-1 text-[10px] uppercase tracking-wider text-zinc-600">lens</span>
              {LENSES.map((l) => (
                <button
                  key={l.id}
                  type="button"
                  onClick={() => setLens(l.id)}
                  aria-pressed={lens === l.id}
                  className={`rounded px-2 py-0.5 text-[11px] ${
                    lens === l.id
                      ? "bg-white/10 text-zinc-100"
                      : "text-zinc-400 hover:bg-white/[0.05] hover:text-zinc-200"
                  }`}
                >
                  {l.label}
                </button>
              ))}
            </div>
          )}
        </div>

        {/*
          `min-h-0` is load-bearing. A column flex item's automatic minimum size is its *content*, so without it
          this pane grew to its content's height (measured: 1933px inside a 813px window) and the scroll
          container inside it never had anything to scroll — everything below the fold was clipped by the shell.
          That is what made the universe look un-centred: its canvas was 946x1742, so the map was correctly framed
          in a box three times taller than the window.
        */}
        <main className="flex min-h-0 min-w-0 flex-1 flex-col">
          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3">
            <Suspense fallback={<RoomLoading />}>
            {functionsError != null && (
              <Unavailable
                what="The function index could not be read"
                why={String(functionsError)}
              />
            )}
            {room === "repo" && (
              <RepoRoom functions={list} loading={functionsLoading} onGo={goRoom} onPick={pick} />
            )}
            {room === "understand" && staleFocus && (
              <Unavailable
                what="This Focus does not resolve against the current bundle"
                why={`The URL carries a subject this index does not contain. That is expected after a re-index if the symbol was removed or renamed — the key is derived from the name, so a rename produces a different key. Pick a new subject with ⌘K, or clear the Focus.`}
              />
            )}
            {room === "understand" && !staleFocus && (
              <UnderstandRoom
                entry={entry}
                lens={lens}
                loading={functionsLoading}
                onOpenPalette={() => setPaletteOpen(true)}
                onCarry={goRoom}
              />
            )}
            {room === "architecture" && (
              <ArchitectureRoom
                // The open district rides the URL with the Focus, so opening a symbol from inside a level and
                // coming back returns to that level rather than to the universe.
                explode={explode}
                onExplodeChange={setExplode}
                onOpenFocus={(focus) => {
                  // The shell owns the Focus: write the key, then move to the room that renders it.
                  setFocus(focus);
                  goRoom("understand");
                }}
              />
            )}
            {room === "migration" && <MigrationRoom />}
            {room === "taint" && <TaintRoom />}
            {room !== "repo" &&
              room !== "understand" &&
              room !== "architecture" &&
              room !== "migration" &&
              room !== "taint" && <Placeholder room={room} />}
            </Suspense>
          </div>
        </main>
      </div>

      <Palette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        functions={list}
        functionsLoading={functionsLoading}
        manifest={manifest.data ?? null}
        onPick={pick}
        onPickKey={pickKey}
      />
    </div>
  );
}

function RoomLoading() {
  return (
    <Panel title="Loading room…">
      <p className="text-[11px] text-zinc-500">Loading the room chunk and its data…</p>
    </Panel>
  );
}

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

/**
 * Orientation: exactly one screen, and it links into work rather than printing totals. Totals without a
 * destination are what made the old landing experience a wall of numbers.
 */
function RepoRoom({
  functions,
  loading,
  onGo,
  onPick,
}: {
  functions: FunctionEntry[];
  loading: boolean;
  onGo: (r: RoomId) => void;
  onPick: (f: FunctionEntry) => void;
}) {
  const withPdg = functions.filter((f) => (f.pdg_nodes ?? 0) > 0);
  const densest = [...withPdg].sort((a, b) => (b.pdg_nodes ?? 0) - (a.pdg_nodes ?? 0)).slice(0, 3);

  return (
    <div className="flex flex-col gap-3">
      <Panel title="Repo" subtitle="where do I start?">
        <p className="text-[12px] leading-relaxed text-zinc-400">
          This screen is not a dashboard. It ranks the few things worth attention and links into the room
          that handles each one. Totals live in the graph — they do not tell you where to look.
        </p>
      </Panel>

      {loading && (
        <Panel title="Loading the bundle…">
          <p className="text-[11px] text-zinc-500">reading indexes…</p>
        </Panel>
      )}

      {!loading && (
        <>
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
            <button type="button" onClick={() => onGo("understand")} className="text-left">
              <Panel title={`${withPdg.length} symbols carry data-flow`} subtitle="understand →">
                <p className="text-[11px] text-zinc-400">
                  Each opens in Understand with its source and CFG. Start where the code is densest:
                </p>
                <ul className="mt-2 space-y-0.5">
                  {densest.map((f) => (
                    <li key={f.function_id} className="font-mono text-[11px] text-zinc-300">
                      {f.name} <span className="text-zinc-600">· {f.pdg_nodes} pdg</span>
                    </li>
                  ))}
                </ul>
              </Panel>
            </button>

            <button type="button" onClick={() => onGo("proof")} className="text-left">
              <Panel title="Why trust any of this?" subtitle="proof →">
                <p className="text-[11px] text-zinc-400">
                  The speed matrix, correctness scorecard and published limits, with the commands behind
                  every number.
                </p>
              </Panel>
            </button>
          </div>

          <Panel title="Pick a subject" subtitle="or press ⌘K from anywhere">
            <div className="flex flex-wrap gap-1.5">
              {functions.slice(0, 12).map((f) => (
                <button
                  key={f.function_id}
                  type="button"
                  onClick={() => onPick(f)}
                  title={keyOf(f)}
                  className="rounded border border-white/12 bg-white/[0.03] px-2 py-0.5 font-mono text-[11px] text-zinc-300 hover:border-sky-400/40 hover:text-sky-200"
                >
                  {f.name}
                </button>
              ))}
            </div>
          </Panel>
        </>
      )}
    </div>
  );
}

function UnderstandRoom({
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
