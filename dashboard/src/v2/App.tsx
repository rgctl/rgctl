import { useEffect } from "react";
import { useQuery } from "@tanstack/react-query";
import { getRouteApi } from "@tanstack/react-router";
import { Instrument } from "./Instrument";
import { Palette } from "./Palette";
import { Chip, Panel, Unavailable } from "./shell";
import { Spine } from "./rooms/Spine";
import { CfgPdgLens } from "./rooms/CfgPdg";
import { LensPlaceholder, Placeholder } from "./rooms/Placeholder";
import { loadFunctions, loadManifest, type FunctionEntry } from "./data";
import { LENSES, ROOMS, isLens, isRoom, keyOf, resolveFocus, type LensId, type RoomId } from "./focus";
import { useState } from "react";

const roomApi = getRouteApi("/$roomId");

export function App() {
  const params = roomApi.useParams();
  const search = roomApi.useSearch();
  const navigate = roomApi.useNavigate();

  const room = (isRoom(params.roomId) ? params.roomId : "repo") as RoomId;
  const lens = (isLens(search.lens) ? search.lens : "spine") as LensId;
  const focus = search.focus ?? null;

  const [paletteOpen, setPaletteOpen] = useState(false);

  const manifest = useQuery({ queryKey: ["manifest"], queryFn: loadManifest });
  const functions = useQuery({ queryKey: ["functions"], queryFn: loadFunctions });

  const list = functions.data ?? [];
  // A Focus that no longer resolves is a real state (a stale link, or a symbol that was removed), and the
  // shell says so rather than silently showing no subject.
  const entry: FunctionEntry | null = focus ? resolveFocus(focus, list) : null;
  const staleFocus = focus !== null && !entry && !functions.isLoading;

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

  const setFocus = (key: string | null) =>
    void navigate({ search: (prev) => ({ ...prev, focus: key ?? undefined }), replace: false });

  /**
   * Picking a symbol sets the Focus, and leaves the orientation surface if that is where you are.
   *
   * The intent is that the graph is one action from the landing page. Setting only the Focus left Repo on
   * screen with a subject it does not render, so the graph stayed two actions away — pick, then guess that
   * Understand is where it lives. Rooms that read the Focus are left alone: choosing a symbol while in
   * Security should not drag you out of Security.
   */
  const pick = (f: FunctionEntry) => {
    setPaletteOpen(false);
    setFocus(keyOf(f));
    if (room === "repo") {
      void navigate({ to: "/$roomId", params: { roomId: "understand" }, search: (prev) => ({ ...prev, focus: keyOf(f) }) });
    }
  };

  const goRoom = (next: RoomId) =>
    void navigate({ to: "/$roomId", params: { roomId: next }, search: (prev) => prev });

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

      <div className="flex min-h-0 flex-1">
        <nav aria-label="Rooms" className="w-52 shrink-0 overflow-y-auto border-r border-white/8 bg-[#0c0f13] p-2">
          <div className="px-2 pb-1.5 text-[10px] uppercase tracking-wider text-zinc-600">rooms</div>
          {ROOMS.map((r) => (
            <button
              key={r.id}
              type="button"
              onClick={() => goRoom(r.id)}
              aria-current={room === r.id ? "page" : undefined}
              className={`mb-0.5 block w-full rounded px-2 py-1.5 text-left ${
                room === r.id ? "bg-sky-500/12 text-sky-100" : "text-zinc-300 hover:bg-white/[0.04]"
              }`}
            >
              <span className="block text-[12px]">{r.label}</span>
              <span className="block truncate text-[10px] text-zinc-500">{r.blurb}</span>
            </button>
          ))}
        </nav>

        <main className="flex min-w-0 flex-1 flex-col">
          <div className="flex shrink-0 flex-wrap items-center gap-1 border-b border-white/8 bg-[#0e1116] px-3 py-1.5">
            <span className="mr-1 text-[10px] uppercase tracking-wider text-zinc-600">lens</span>
            {LENSES.map((l) => (
              <button
                key={l.id}
                type="button"
                onClick={() => void navigate({ search: (prev) => ({ ...prev, lens: l.id }) })}
                aria-pressed={lens === l.id}
                className={`rounded px-2 py-0.5 text-[11px] ${
                  lens === l.id ? "bg-white/10 text-zinc-100" : "text-zinc-400 hover:bg-white/[0.05] hover:text-zinc-200"
                }`}
              >
                {l.label}
              </button>
            ))}
            <span className="ml-auto flex items-center gap-2 text-[10px] text-zinc-600">
              <span>{ROOMS.find((r) => r.id === room)?.label}</span>
              {entry && <Chip tone="blue">subject: {entry.name}</Chip>}
            </span>
          </div>

          <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-3">
            {functions.isError && (
              <Unavailable
                what="The function index could not be read"
                why={String(functions.error)}
              />
            )}
            {room === "repo" && <RepoRoom functions={list} loading={functions.isLoading} onGo={goRoom} onPick={pick} />}
            {room === "understand" && staleFocus && (
              <Unavailable
                what="This Focus does not resolve against the current bundle"
                why={`The URL carries a subject this index does not contain. That is expected after a re-index if the symbol was removed or renamed — the key is derived from the name, so a rename produces a different key. Pick a new subject with ⌘K, or clear the Focus.`}
              />
            )}
            {room === "understand" && !staleFocus && (
              <UnderstandRoom entry={entry} lens={lens} loading={functions.isLoading} onOpenPalette={() => setPaletteOpen(true)} onCarry={goRoom} />
            )}
            {room !== "repo" && room !== "understand" && <Placeholder room={room} />}
          </div>
        </main>
      </div>

      <Palette
        open={paletteOpen}
        onOpenChange={setPaletteOpen}
        functions={list}
        onPick={pick}
      />
    </div>
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

      {loading && <Panel title="Loading the bundle…"><p className="text-[11px] text-zinc-500">reading indexes…</p></Panel>}

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
      <Panel
        title={entry.name}
        subtitle={entry.file_path ?? undefined}
        right={<Chip tone="blue">focus</Chip>}
      >
        <div className="flex flex-wrap items-center gap-2">
          <span className="text-[10px] uppercase tracking-wider text-zinc-500">carry this focus to</span>
          {(["review", "security", "architecture", "migration", "proof"] as RoomId[]).map((r) => (
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

      {lens === "spine" && <Spine entry={entry} />}
      {lens === "code" && (
        <Panel title="Code" subtitle={`${entry.file_path ?? "—"}`}>
          <p className="text-[11px] text-zinc-400">
            The Code lens is the same source pane as Spine without the control-flow graph beside it.
            Source rendering is shared, so this lens is a view of the same reader rather than a second
            implementation.
          </p>
        </Panel>
      )}
      {lens === "cfgpddg" && <CfgPdgLens entry={entry} />}
      {lens !== "spine" && lens !== "code" && lens !== "cfgpddg" && <LensPlaceholder lens={lens} />}
    </div>
  );
}
