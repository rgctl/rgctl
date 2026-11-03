/**
 * The Architecture room — the universe's first two layers.
 *
 * The room leads with what the map *is* and what it leaves out, before it draws anything: how much of the
 * graph the structure covers and which evidence said so, that a volume is a namespace the room derived and a
 * solid is a package the export called a metagraph node, that colour means "shares its community" rather than
 * identity, and that the two biggest objects in the export are buckets rather than code.
 *
 * It renders without a Focus and without a lens: a package is a set of functions, not a function, so there is
 * nothing here to carry in or out of another room.
 */
import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { Panel, Unavailable, Chip } from "../shell";
import { channelReport, loadArchitecture, scalesFor } from "../architecture";
import { expandMembers, MEMBER_EDGE_KINDS } from "../expandEngine";
import { loadMemberIndices } from "../metagraphMembers";
import { loadFunctions, type FunctionEntry } from "../data";
import { STALE_CHUNK_MESSAGE, isStaleChunkError } from "../staleChunk";
import { SymbolReader } from "./SymbolReader";
import type { ArchitectureState, ArchitectureView, Metagraph, PackageNode } from "../architectureTypes";
import type { HoverEntry, MemberEntry, UniverseLayer, UniverseScene } from "../universe/UniverseScene";

type Loaded = { kind: "loading" } | ArchitectureState;

/** One member as the level carries it: the engine's own fields, and nothing invented. */
interface MemberNode {
  index: number;
  name: string;
  kind: string;
  filePath: string | null;
}

/**
 * A row of the level's list, as a scene pick.
 *
 * The list carries no instance slot — it is derived from the subgraph payload rather than from the drawn array —
 * so the scene resolves the slot from the graph index, which is what the level indexed it under.
 */
function memberOf(node: MemberNode): MemberEntry {
  return { index: node.index, name: node.name, nodeTypeName: node.kind, filePath: node.filePath };
}

/** An import failure that means "reload" versus one that means "this browser cannot". */
function staleChunkFailure(error: unknown): { reason: string; stale: boolean } {
  if (isStaleChunkError(error)) return { reason: STALE_CHUNK_MESSAGE, stale: true };
  return { reason: (error as Error)?.message ?? String(error), stale: false };
}

export function ArchitectureRoom({
  onOpenFocus,
  explode = null,
  onExplodeChange,
}: {
  /**
   * Carry a symbol to another room.
   *
   * Supplied by the shell, which owns the Focus (`setFocus` + `goRoom` from `useFocus`). A double-clicked member
   * opens in this room's own drawer, because leaving the level to read one function throws away the context the
   * reader is holding; the drawer offers this as a secondary action for readers who want the whole room. The room
   * still does not reach for the router itself — it is rendered standalone in tests.
   */
  onOpenFocus?: (functionId: string) => void;
  /**
   * The district whose level is open, from the URL — the room's level is a link.
   *
   * Two jobs at once: a level someone can share, and the only way a *return* works. Opening a member leaves the
   * room, which unmounts this component and its state; without the URL the reader comes back to the universe and
   * has to find the district again.
   */
  explode?: number | null;
  onExplodeChange?: (id: number | null) => void;
} = {}) {
  const [state, setState] = useState<Loaded>({ kind: "loading" });
  const [layer, setLayer] = useState<UniverseLayer>("packages");
  const [edges, setEdges] = useState(true);
  const [sharedOnly, setSharedOnly] = useState(false);
  /** Drift is off by default once a district has been opened; the reader can turn it back on. */
  const [drift, setDrift] = useState(true);
  /** What happened when a member was clicked: picked (a name, no navigation), resolving, or why it failed. */
  const [memberState, setMemberState] = useState<
    | { kind: "idle" }
    | { kind: "picked"; member: MemberEntry }
    | { kind: "resolving"; member: MemberEntry }
    | { kind: "failed"; member: MemberEntry; reason: string }
  >({ kind: "idle" });
  const [hover, setHover] = useState<HoverEntry | null>(null);
  /** The clicked object. Hover is a preview; expanding acts on a selection, so a control can carry it. */
  const [selected, setSelected] = useState<HoverEntry | null>(null);
  /**
   * The district currently opened, and how its members were obtained.
   *
   * `drawn` is separate from `members` on purpose: a district with more members than `EXPLODE_MAX` must say
   * how many it drew and by what rule, and a truthy "expanded" flag with no number is how a truncation
   * becomes invisible.
   */
  const [expanded, setExpanded] = useState<{
    id: number;
    title: string;
    members: number;
    drawn: number;
    edges: number;
    truncated: boolean;
    /** Milliseconds from the click to the district being drawn — a number, not the word "smooth". */
    ms: number;
    /** False when the bundle carries no per-member complexity, so members are drawn at a constant height. */
    measuredHeight: boolean;
    /** Aggregate edges leaving this district, drawn outward and fading — package-level, not member-level. */
    boundary: number;
    /**
     * The members that were drawn, in the order the engine returned them.
     *
     * Kept because the right column lists the level instead of the universe: a district of 1 500 members has
     * 1 500 names, and finding one by sweeping a pointer across a 3D field is not a way to find it.
     */
    nodes: MemberNode[];
  } | null>(null);
  const [expandState, setExpandState] = useState<
    | { kind: "idle" }
    | {
        kind: "loading";
        title: string;
        phase: "members" | "engine" | "parsing" | "expanding";
        loaded?: number;
        total?: number;
      }
    | { kind: "absent"; title: string; reason: string }
    | { kind: "failed"; title: string; reason: string }
  >({ kind: "idle" });
  /**
   * Why the canvas is not there, and whether reloading is the fix.
   *
   * `stale` is the difference between "this browser cannot draw" (nothing to do) and "this page is older than
   * the bundle on the server" (one click). Both are stated; only one is actionable.
   */
  /**
   * The symbol open beside the level, and whether it has the room to itself.
   *
   * A double-click inside a district opens the member's code here rather than leaving the room.
   */
  const [symbol, setSymbol] = useState<FunctionEntry | null>(null);
  const [symbolMax, setSymbolMax] = useState(false);
  const [sceneFailed, setSceneFailed] = useState<{ reason: string; stale: boolean } | null>(null);
  /** False until the canvas exists — an effect that draws must wait for something to draw on. */
  const [sceneReady, setSceneReady] = useState(false);

  const hostRef = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<UniverseScene | null>(null);

  useEffect(() => {
    let alive = true;
    void loadArchitecture()
      .then((next) => { if (alive) setState(next); })
      .catch((error: unknown) => {
        if (alive) setState({ kind: "unreadable", reason: (error as Error).message });
      });
    return () => { alive = false; };
  }, []);

  const view = state.kind === "ready" || state.kind === "no-communities" ? state.view : null;
  // The metagraph is what membership is addressed against; `view` carries the derived packages only.
  const metagraph: Metagraph | null =
    state.kind === "ready" || state.kind === "no-communities" ? state.metagraph : null;
  // The scene is created once and holds its callbacks, so it must call through a ref to reach the current
  // closure — otherwise a district opened after a re-render would use a stale metagraph or a stale ceiling.
  const expandRef = useRef<(entry: HoverEntry | null) => Promise<void>>(async () => {});
  const driftRef = useRef(drift);
  const openSymbolRef = useRef<(member: MemberEntry | null) => Promise<void>>(async () => {});
  const pickMemberRef = useRef<(member: MemberEntry | null) => void>(() => {});

  /**
   * How many members a district will draw.
   *
   * The largest district on the measured corpus holds 15 686 members; drawing them all is a frame-rate cliff
   * and an unreadable cloud, so the ceiling binds — and when it does, the view says so rather than quietly
   * showing a sample.
   */
  const EXPLODE_MAX = 1500;

  const collapse = useCallback(() => {
    sceneRef.current?.collapse();
    sceneRef.current?.highlightSolid(null);
    setExpanded(null);
    setExpandState({ kind: "idle" });
    setMemberState({ kind: "idle" });
    // Back at the universe level nothing is selected and no symbol is open: leaving the selection set kept
    // Expand painted as though it were still armed, against a district that is no longer open.
    setSelected(null);
    setSymbol(null);
    setSymbolMax(false);
    // The level is a link, so closing it is a URL write like opening it was.
    onExplodeChange?.(null);
  }, [onExplodeChange]);

  /** What the room says about a district, in one place: the list, the lookup and the URL all use it. */
  const selectionOf = useCallback(
    (pkg: PackageNode): HoverEntry => ({
      kind: "package",
      id: pkg.id,
      title: pkg.label,
      lines: [
        `${pkg.functions.toLocaleString()} functions · ${pkg.classes.toLocaleString()} classes · ${pkg.size.toLocaleString()} members`,
        `community: ${pkg.communityLabel}`,
      ],
    }),
    [],
  );

  /**
   * What choosing a district from a list means.
   *
   * Both lists are the universe's own index, so choosing a row is a request for the *universe* level: an open
   * district is closed first (otherwise the camera flies to a coordinate the open level no longer shows), and
   * if the reader is on the aggregated layer the room switches to the layer those packages are drawn on —
   * otherwise the row highlights and the box it names is not on the screen. The flight is queued rather than
   * issued in that case, because the solids it aims at do not exist until the scene has rebuilt.
   */
  const pendingFocus = useRef<number | null>(null);

  const pickDistrict = useCallback(
    (pkg: PackageNode) => {
      setSelected(selectionOf(pkg));
      if (expanded) collapse();
      if (layer === "packages") {
        sceneRef.current?.focusSolid(pkg.id);
        return;
      }
      pendingFocus.current = pkg.id;
      setLayer("packages");
    },
    [collapse, expanded, layer, selectionOf],
  );

  const expand = useCallback(
    async (entry: HoverEntry | null) => {
      if (!entry || entry.id === undefined) return;
      if (entry.kind !== "package") {
        setExpandState({
          kind: "absent",
          title: entry.title,
          reason:
            entry.kind === "volume"
              ? "A namespace group is derived from package labels, not an exported object — it has no members of its own. Open a package inside it."
              : "This object has no member list in the bundle.",
        });
        return;
      }
      if (!metagraph) {
        setExpandState({
          kind: "failed",
          title: entry.title,
          reason:
            "The metagraph is not loaded, so membership cannot be addressed. That is a wiring failure, not an empty district.",
        });
        return;
      }
      setExpandState({ kind: "loading", title: entry.title, phase: "members" });
      sceneRef.current?.collapse();
      setExpanded(null);

      const members = await loadMemberIndices(metagraph, entry.id);
      if (members.kind === "absent" || members.kind === "failed") {
        setExpandState({ kind: members.kind, title: entry.title, reason: members.reason });
        return;
      }
      if (members.kind !== "ready") return;

      const startedAt = performance.now();
      const total = members.indices.length;
      const truncated = total > EXPLODE_MAX;
      const kept = truncated ? members.indices.slice(0, EXPLODE_MAX) : members.indices;

      try {
        const subgraph = await expandMembers(kept, 0, (progress) => {
          setExpandState({
            kind: "loading",
            title: entry.title,
            phase: progress.phase,
            loaded: progress.loaded,
            total: progress.total,
          });
        });
        setExpandState({ kind: "loading", title: entry.title, phase: "expanding" });
        const result = sceneRef.current?.expand(
          entry.id,
          subgraph.nodes.map((n) => ({
            index: n.index,
            complexity: n.complexity,
            name: n.name,
            nodeTypeName: n.node_type_name,
            filePath: n.file_path ?? null,
          })),
          subgraph.edges,
        );
        setExpanded({
          id: entry.id,
          title: entry.title,
          members: total,
          drawn: result?.drawn ?? 0,
          edges: result?.edges ?? 0,
          truncated,
          ms: Math.round(performance.now() - startedAt),
          measuredHeight: result?.measuredHeight ?? false,
          boundary: result?.boundary ?? 0,
          nodes: subgraph.nodes.map((n) => ({
            index: n.index,
            name: n.name,
            kind: n.node_type_name,
            filePath: n.file_path ?? null,
          })),
        });
        setExpandState({ kind: "idle" });
        setSelected(entry);
        // A level is a link: the open district goes into the URL, which is also what brings the reader back
        // to this level after opening a symbol inside it.
        onExplodeChange?.(entry.id);
      } catch (error) {
        setExpandState({ kind: "failed", title: entry.title, reason: (error as Error).message });
      }
    },
    [metagraph, onExplodeChange],
  );

  /**
   * Restore an open district from the URL.
   *
   * This is what makes "double-click a member, read the code, come back" land on the level rather than the
   * universe. The district's *existence* is checked before the canvas is, so a stale link is reported as a stale
   * link even where nothing can be drawn; `attempted` stops a failing link from retrying on every render.
   */
  const attempted = useRef<number | null>(null);
  useEffect(() => {
    if (explode === null || explode === undefined) {
      attempted.current = null;
      return;
    }
    if (!view || expanded?.id === explode) return;
    const pkg = view.packages.find((p) => p.id === explode);
    if (!pkg) {
      // Checked against the data, before the canvas: a stale link is a stale link even where nothing draws.
      if (attempted.current === explode) return;
      attempted.current = explode;
      setExpandState({
        kind: "absent",
        title: `district #${explode}`,
        reason:
          "The URL names a district that is not in this bundle, so there is nothing to open. The link is stale; the map is not empty.",
      });
      return;
    }
    if (!sceneReady || attempted.current === explode) return;
    attempted.current = explode;
    setLayer("packages");
    setSelected(selectionOf(pkg));
    void expand(selectionOf(pkg));
  }, [explode, expanded, expand, sceneReady, selectionOf, view]);


  useEffect(() => {
    driftRef.current = drift;
    sceneRef.current?.setDrift(drift);
  }, [drift]);

  // Escape closes the district: the universal "go back a level", and cheaper than aiming at a solid.
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === "Escape" && sceneRef.current?.isExpanded) collapse();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [collapse]);

  /**
   * Turn a double-clicked member into a Focus.
   *
   * The engine's subgraph payload carries `(file_path, name)` but not the node's UUID, so the identity is
   * resolved against the dashboard's own function index — the same index the ⌘K palette uses. An ambiguous or
   * missing match is **not** guessed: a Focus that points at the wrong function is worse than no link at all.
   *
   * The district is deliberately left open: the level is in the URL, so coming back returns to it instead of to
   * a universe the reader has to re-navigate. Closing it here was the old behaviour and it threw away the
   * reader's place for no reason.
   */
  const openSymbol = useCallback(async (member: MemberEntry | null) => {
    if (!member) return;
    setMemberState({ kind: "resolving", member });
    const functions = await loadFunctions();
    const matches = functions.filter(
      (f) => f.name === member.name && (!member.filePath || f.file_path === member.filePath),
    );
    if (matches.length === 1) {
      setMemberState({ kind: "idle" });
      setSymbol(matches[0]);
      return;
    }
    setMemberState({
      kind: "failed",
      member,
      reason:
        matches.length === 0
          ? "no indexed function matches this member's name and file, so there is no source or CFG to open — the CFG index does not reach it"
          : `${matches.length} indexed functions match this name and file, so opening one would be a guess`,
    });
  }, []);

  /**
   * A click on a member *names* it; it does not navigate, and it no longer prints a paragraph.
   *
   * The hover tooltip already gives the name, the kind and the file, and the level's list marks the same member —
   * so a second read-out on the canvas was noise over the thing being read. What the click adds is the **mark**:
   * a glow on the box, which is the part hovering cannot leave behind.
   */
  const pickMember = useCallback((member: MemberEntry | null) => {
    setMemberState(member ? { kind: "picked", member } : { kind: "idle" });
    sceneRef.current?.highlightMember(member);
  }, []);
  useEffect(() => {
    openSymbolRef.current = openSymbol;
  }, [openSymbol]);
  useEffect(() => {
    pickMemberRef.current = pickMember;
  }, [pickMember]);
  useEffect(() => {
    pickMemberRef.current = pickMember;
  }, [pickMember]);
  // The canvas is a canvas: jsdom has no WebGL, and neither does a browser with it switched off. Both get a
  // stated absence instead of a blank pane, and everything textual above still renders.
  useEffect(() => {
    if (!view) return;
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    let scene: UniverseScene | null = null;
    let observer: ResizeObserver | null = null;

    // Loaded on demand: three.js is the largest thing in this bundle and only this room needs it, so the
    // shell's first paint should not pay for a canvas nobody has opened yet.
    void (async () => {
      let module: typeof import("../universe/UniverseScene");
      try {
        module = await import("../universe/UniverseScene");
      } catch (error) {
        setSceneFailed(staleChunkFailure(error));
        return;
      }
      if (disposed) return;
      if (!module.webglAvailable()) {
        setSceneFailed({ reason: "this browser could not create a WebGL context", stale: false });
        return;
      }
      try {
        const created = new module.UniverseScene(host);
        scene = created;
        created.setDrift(driftRef.current);
        created.onHoverChange(setHover);
        // A press in the scene marks and flies itself (behind the double-click window), so the room only keeps
        // the selection for its read-out and for Expand. Lists still go through `focusSolid`.
        created.onPickChange(setSelected);
        created.onMemberPickChange((member) => pickMemberRef.current(member));
        created.onMemberOpen((member) => void openSymbolRef.current(member));
        created.onExplode((entry) => void expandRef.current(entry));
        created.attachPointer(host);
        const scales = scalesFor(view);
        created.setEdges(
          edgeListFor(view),
          new Map(view.packages.map((p) => [p.id, p])),
        );
        created.update({ view, layer, edges, height: scales.height, footprint: scales.footprint });
        sceneRef.current = created;
        if (!disposed) setSceneReady(true);
        observer = new ResizeObserver(() => created.resize(host));
        observer.observe(host);
      } catch (error) {
        setSceneFailed(staleChunkFailure(error));
        return;
      }
    })();

    return () => {
      disposed = true;
      setSceneReady(false);
      observer?.disconnect();
      scene?.dispose();
      sceneRef.current = null;
    };
    // `layer`/`edges` are applied by the effect below, so this one only owns the scene's lifetime
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [view]);

  useEffect(() => {
    const scene = sceneRef.current;
    if (!scene || !view) return;
    const scales = scalesFor(view);
    scene.setEdges(
      edgeListFor(view),
      new Map(view.packages.map((p) => [p.id, p])),
    );
    scene.update({ view, layer, edges, height: scales.height, footprint: scales.footprint });
    // Declared after the update above, so a queued district is aimed at *after* the rebuild it waited for.
    const queued = pendingFocus.current;
    if (queued !== null) {
      pendingFocus.current = null;
      scene.focusSolid(queued);
    }
  }, [view, layer, edges]);

  if (state.kind === "loading") {
    return (
      <Panel title="Architecture" subtitle="reading the bundle">
        <p className="text-[11px] text-zinc-500">Reading `metagraph.json` and `communities.json`…</p>
      </Panel>
    );
  }

  if (state.kind === "no-metagraph") {
    return (
      <Panel title="Architecture" subtitle="no metagraph in this bundle">
        <Unavailable
          tag="not exported"
          what="This bundle carries no architecture dataset"
          why="The export produced no `metagraph.json`, so there is nothing to draw. It is written by `rgctl discover . --with-universe --with-cfg`; a bundle indexed without it renders no city rather than an empty one."
        />
      </Panel>
    );
  }

  if (state.kind === "unreadable") {
    return (
      <Panel title="Architecture" subtitle="the dataset could not be read">
        <Unavailable tag="unreadable" what="The architecture dataset could not be parsed" why={state.reason} />
      </Panel>
    );
  }

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2">
      <Coverage view={view!} />
      {state.kind === "no-communities" && (
        <Unavailable
          tag="clustering absent"
          what="Clustering was not applied to this bundle"
          why="`communities.json` is missing or empty, so there is no community to show as colour, as a list, or as an aggregate. The packages and their coupling are unaffected — this is not 'one big community'."
        />
      )}
      {/*
        Three things can share this row: the symbol drawer, the canvas, and the level's own column. Maximizing the
        drawer is the one case where one thing deserves the whole width, so the canvas and the column step aside
        rather than being squeezed — a half-width CFG beside a half-width universe is the worst of both.
      */}
      <div className={`grid min-h-0 flex-1 gap-2 ${symbolMax ? "grid-cols-1" : "lg:grid-cols-[minmax(0,1fr)_300px]"}`}>
        <div className="flex min-h-0 min-w-0 gap-2">
          {symbol && (
            <SymbolDrawer
              entry={symbol}
              maximized={symbolMax}
              onToggleMaximize={() => setSymbolMax((value) => !value)}
              onClose={() => {
                setSymbol(null);
                setSymbolMax(false);
              }}
              onOpenInRoom={onOpenFocus ? () => onOpenFocus(symbol.function_id) : undefined}
            />
          )}
          {!symbolMax && (
        <div className="relative min-h-[280px] min-w-0 flex-1 overflow-hidden rounded-lg border border-white/8 bg-[#060a12]">
          <div ref={hostRef} className="absolute inset-0" data-testid="universe-canvas" />
          {sceneFailed && (
            <div className="absolute inset-0 flex items-center justify-center p-4">
              <div className="flex flex-col items-start gap-2">
                <Unavailable
                  tag={sceneFailed.stale ? "older build" : "drawing unavailable"}
                  what={sceneFailed.stale ? "This page is older than the bundle on the server" : "The drawing could not be started"}
                  why={`${sceneFailed.reason} The coverage, the layers and every list beside this panel are unaffected — a failure of the canvas is not a failure of the room.`}
                />
                {sceneFailed.stale && (
                  // The only action that can work: the chunks this page names are gone, so it has to come back
                  // for a new entry script. Said as a button rather than as advice.
                  <button
                    type="button"
                    onClick={() => globalThis.location?.reload()}
                    className="rounded-md bg-sky-500 px-2.5 py-1 text-[11px] font-medium leading-none text-slate-950 hover:bg-sky-400"
                  >
                    Reload the dashboard
                  </button>
                )}
              </div>
            </div>
          )}
          <div className="pointer-events-none absolute left-2 top-2 flex gap-1">
            <Chip tone={layer === "packages" ? "blue" : "zinc"}>
              {layer === "packages" ? "namespace › package" : "communities (aggregated)"}
            </Chip>
            <Chip tone="zinc">{edges ? "coupling drawn" : "coupling hidden"}</Chip>
          </div>
          {hover && (
            <div className="pointer-events-none absolute bottom-2 left-2 max-w-[420px] rounded border border-white/12 bg-black/80 p-2">
              <div className="text-[11px] font-semibold text-zinc-100">{hover.title}</div>
              {hover.lines.map((line) => (
                <div key={line} className="text-[10.5px] leading-relaxed text-zinc-400">{line}</div>
              ))}
            </div>
          )}
          {/* The controls are the discoverable half of the gesture: double-click works, but a gesture nobody
              knows about is not an affordance. They are sized and weighted like real actions rather than
              hints, because a 10px ghost button next to a 3D scene is invisible in practice. */}
          <div className="absolute left-2 top-9 flex max-w-[calc(100%-330px)] flex-col gap-1.5">
            <DistrictSearch packages={view!.packages} onPick={pickDistrict} />
            <div className="flex flex-wrap items-center gap-1.5">
              <button
                type="button"
                disabled={selected?.kind !== "package" || selected.id === undefined || expandState.kind === "loading"}
                onClick={() => void expand(selected)}
                title={
                  selected?.kind === "package"
                    ? `Open ${selected.title} — its members and the calls between them`
                    : "Select a district first (click it in the scene), then expand"
                }
                className={`flex items-center gap-1 rounded-md px-2.5 py-1 text-[11px] font-medium leading-none shadow-sm transition-colors ${
                  selected?.kind === "package"
                    ? "bg-sky-500 text-slate-950 hover:bg-sky-400"
                    : "cursor-not-allowed border border-white/10 bg-white/[0.04] text-zinc-500"
                }`}
              >
                <span aria-hidden>⤢</span>
                <span>Expand{districtLabel(selected) ? `: ${districtLabel(selected)}` : " district"}</span>
              </button>
              <button
                type="button"
                disabled={!expanded}
                onClick={collapse}
                title="Close the district and restore the universe (Esc)"
                className={`flex items-center gap-1 rounded-md px-2.5 py-1 text-[11px] font-medium leading-none shadow-sm transition-colors ${
                  expanded
                    ? "bg-white/[0.14] text-zinc-100 ring-1 ring-white/25 hover:bg-white/[0.22]"
                    : "cursor-not-allowed border border-white/10 bg-white/[0.04] text-zinc-500"
                }`}
              >
                <span aria-hidden>⤡</span>
                <span>Collapse</span>
                <kbd className="ml-0.5 rounded border border-white/20 px-1 text-[9px] font-normal text-zinc-400">
                  esc
                </kbd>
              </button>
            </div>
            {expandState.kind === "idle" && !expanded && (
              <span className="text-[10px] leading-none text-zinc-500">
                {selected?.kind === "package"
                  ? "double-click a district, or select it and press Expand"
                  : "click a district in the scene to select it"}
              </span>
            )}
            {expandState.kind === "loading" && (
              <ProgressPanel
                title={expandState.title}
                phase={expandState.phase}
                loaded={expandState.loaded}
                total={expandState.total}
              />
            )}
            {expandState.kind === "absent" && (
              <Chip tone="zinc">
                {expandState.title}: {expandState.reason}
              </Chip>
            )}
            {expandState.kind === "failed" && (
              <Chip tone="zinc">
                {expandState.title}: {expandState.reason}
              </Chip>
            )}
            {expanded && (
              <span className="text-[10px] leading-none text-zinc-500">
                click a box for its name, double-click for its code
              </span>
            )}
          </div>


          {/*
            Top right, not top left: the left edge is where the reader is *doing* things (chips, lookup, Expand),
            and this is a description of what is now on screen. Two lines came off it — the transition time, now
            folded into the counts where it is one number instead of a sentence, and the clicked member's name and
            absolute path, which the hover tooltip and the level's list already say.
          */}
          {expanded && (
            <div className="absolute right-2 top-2 flex max-w-[340px] flex-col gap-0.5 rounded-md border border-sky-500/30 bg-[#0a1420]/95 px-2 py-1.5">
              <span className="text-[11px] font-medium leading-none text-sky-100">{expanded.title}</span>
              <span className="text-[10px] leading-relaxed text-sky-200/80">
                {expanded.drawn.toLocaleString()} of {expanded.members.toLocaleString()} members drawn
                {expanded.truncated ? " (largest by member count; the rest are not drawn)" : ""} ·{" "}
                {expanded.edges.toLocaleString()} member edges ({MEMBER_EDGE_KINDS})
                {expanded.boundary > 0 ? ` · ${expanded.boundary} outgoing coupling edges fading outward` : ""} ·{" "}
                {(expanded.ms / 1000).toFixed(1)} s
              </span>
              <span className="text-[10px] leading-relaxed text-zinc-500">
                the universe is hidden while this level is open — Collapse or esc returns to it, same angle
              </span>
              {!expanded.measuredHeight && (
                <span className="text-[10px] leading-relaxed text-amber-200/80">
                  heights are constant — this bundle reports no per-member complexity
                </span>
              )}
            </div>
          )}

          <div className="absolute bottom-2 right-2 flex gap-1">
            <Toggle on={layer === "packages"} onClick={() => setLayer("packages")}>namespace › package</Toggle>
            <Toggle on={layer === "communities"} onClick={() => setLayer("communities")}>communities</Toggle>
            <Toggle on={edges} onClick={() => setEdges((value) => !value)}>edges {edges ? "on" : "off"}</Toggle>
            <Toggle on={drift} onClick={() => setDrift((value) => !value)}>drift {drift ? "on" : "off"}</Toggle>
          </div>
        </div>
          )}
        </div>
        {!symbolMax && (
          <div className="flex min-h-0 min-w-0 flex-col gap-2 overflow-y-auto">
            {/*
              The column answers "what am I looking at" for the level on screen: the universe's index when the
              universe is what is drawn, and the level's own members when a district is open. The district list
              behind an open level is a list of places the reader is no longer in.
            */}
            {expanded ? (
              <Members
                district={expanded.title}
                nodes={expanded.nodes}
                drawn={expanded.drawn}
                selectedIndex={memberState.kind === "picked" ? memberState.member.index : null}
                onPick={(node) => pickMember(memberOf(node))}
                onOpen={(node) => void openSymbol(memberOf(node))}
              />
            ) : (
              <>
                <Districts
                  view={view!}
                  selectedId={selected?.id ?? null}
                  // This branch only runs when no level is open, so nothing is marked as expanded.
                  expandedId={null}
                  onPick={pickDistrict}
                />
                <Layers view={view!} />
                <Namespaces view={view!} />
                <Communities
                  view={view!}
                  sharedOnly={sharedOnly}
                  onToggleShared={() => setSharedOnly((value) => !value)}
                />
                <Buckets view={view!} />
              </>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

/**
 * What is happening, with a bar only where a bar can be honest.
 *
 * The payload download reports real bytes, so it gets a real percentage. The WASM parse and the expand scan
 * report nothing, so they get an indeterminate bar and a sentence naming the phase — a moving stripe that
 * pretends to be 70% done is the lie this panel exists to avoid.
 */
function ProgressPanel({
  title,
  phase,
  loaded,
  total,
}: {
  title: string;
  phase: "members" | "engine" | "parsing" | "expanding";
  loaded?: number;
  total?: number;
}) {
  const measuring = phase === "engine" && typeof total === "number" && total > 0;
  const fraction = measuring && typeof loaded === "number" ? Math.min(1, loaded / total!) : null;
  const mib = (bytes?: number) => (typeof bytes === "number" ? (bytes / (1024 * 1024)).toFixed(1) : null);
  const label =
    phase === "members"
      ? `reading ${title}'s membership…`
      : phase === "engine"
        ? fraction === null
          ? "loading the graph engine…"
          : `loading the graph engine — ${mib(loaded)} of ${mib(total)} MiB (${Math.round(fraction * 100)}%)`
        : phase === "parsing"
          ? "parsing the graph engine (no progress to report — this is the slow step the first time)"
          : "placing members…";

  return (
    <div className="w-72 rounded-md border border-sky-500/30 bg-sky-500/[0.08] px-2 py-1.5">
      <div className="flex items-baseline justify-between gap-2">
        <span className="text-[11px] font-medium leading-none text-sky-100">{title}</span>
        <span className="text-[10px] tabular-nums leading-none text-sky-200/80">
          {measuring && fraction !== null ? `${Math.round(fraction * 100)}%` : ""}
        </span>
      </div>
      <div
        className="mt-1.5 h-1.5 w-full overflow-hidden rounded-full bg-white/10"
        role="progressbar"
        aria-label={label}
        aria-valuemin={measuring ? 0 : undefined}
        aria-valuemax={measuring ? 100 : undefined}
        aria-valuenow={measuring && fraction !== null ? Math.round(fraction * 100) : undefined}
        aria-valuetext={label}
      >
        {fraction !== null ? (
          <div
            className="h-full rounded-full bg-sky-400 transition-[width] duration-200"
            style={{ width: `${Math.max(2, fraction * 100)}%` }}
          />
        ) : (
          <div className="h-full w-1/3 animate-pulse rounded-full bg-sky-400/70" />
        )}
      </div>
      <div className="mt-1 text-[10px] leading-snug text-sky-200/80">{label}</div>
    </div>
  );
}

/** A short name for a district, so the control says what it will open. */
function districtLabel(entry: HoverEntry | null): string | null {
  if (!entry || entry.kind !== "package") return null;
  return entry.title.length > 28 ? `${entry.title.slice(0, 27)}…` : entry.title;
}

function Toggle({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`rounded border px-1.5 py-0.5 text-[10px] leading-none ${
        on ? "border-sky-500/40 bg-sky-500/12 text-sky-200" : "border-white/10 text-zinc-400 hover:text-zinc-200"
      }`}
    >
      {children}
    </button>
  );
}

function edgeListFor(view: ArchitectureView): { source: number; target: number; weight: number }[] {
  return view.edgeList;
}

/**
 * Coverage first, at every viewport — and it says which evidence produced it, because the two forms of the
 * accounting are different fields and one of them is frequently absent.
 */
function Coverage({ view }: { view: ArchitectureView }) {
  const report = channelReport(view);
  const coverage = view.coverage;
  const disagreement = view.disagreement;
  return (
    <Panel
      title="Coverage"
      subtitle={
        coverage
          ? `${(coverage.covered / Math.max(1, coverage.declared) * 100).toFixed(1)}% of the graph is in this map`
          : "could not be established"
      }
      right={<Chip tone="zinc">{view.mode}</Chip>}
    >
      {coverage ? (
        <p className="text-[11.5px] leading-relaxed text-zinc-300">
          The exported structure accounts for <b className="text-zinc-100">{coverage.covered.toLocaleString()}</b> of{" "}
          <b className="text-zinc-100">{coverage.declared.toLocaleString()}</b> nodes —{" "}
          {(coverage.covered / Math.max(1, coverage.declared) * 100).toFixed(1)}%. The remaining{" "}
          {coverage.uncovered.toLocaleString()} are not in it and are not drawn. Evidence:{" "}
          <code className="text-zinc-400">{coverage.source}</code>
          {coverage.source === "member_count"
            ? " (per-node membership is elided above 50,000 nodes, so the summary's counts are the only accounting in the bundle)"
            : " (per-node membership, this bundle's own form of the same accounting)"}
          .
        </p>
      ) : (
        <p className="text-[11.5px] leading-relaxed text-amber-200/90">
          The bundle carries neither per-node membership nor per-community counts, so coverage{" "}
          <b>could not be established</b>. It is not zero: the map below is a partial view and nothing here says
          how partial.
        </p>
      )}
      <p className="mt-2 text-[10.5px] leading-relaxed text-zinc-500">
        Partition: <b className="text-zinc-300">{disagreement.packages}</b> packages in{" "}
        <code>metagraph.json</code> · <b className="text-zinc-300">{disagreement.communities}</b> communities in{" "}
        <code>communities.json</code>
        {disagreement.migration !== null && (
          <> · <b className="text-zinc-300">{disagreement.migration}</b> in <code>migration_graph.json</code></>
        )}
        . Three partitions of one graph, answering different questions — none of them is chosen as authoritative
        here. The largest single solid holds {report.sizeMax.toLocaleString()} members,{" "}
        {report.skew}× the median package.
      </p>
    </Panel>
  );
}

/** The three objects, named. Getting this wrong is the failure that makes the whole view dishonest. */
/**
 * The same finder, in the overlay beside Expand.
 *
 * The right column carries the full list, but that column can be pushed off-screen by a narrow window — as it
 * was when this was first added, leaving "how do I find the filter?" as a fair question. The control beside
 * Expand cannot be clipped, so the search lives in both places.
 */
function DistrictSearch({
  packages,
  onPick,
}: {
  packages: PackageNode[];
  onPick: (pkg: PackageNode) => void;
}) {
  const [query, setQuery] = useState("");
  const view = useMemo(
    () => [...packages].sort((a, b) => b.size - a.size || a.label.localeCompare(b.label)),
    [packages],
  );
  const [open, setOpen] = useState(false);
  const needle = query.trim().toLowerCase();
  const matches = needle
    ? view.filter((p) => p.label.toLowerCase().includes(needle)).slice(0, 6)
    : view.slice(0, 6);

  const pick = (pkg: PackageNode) => {
    onPick(pkg);
    setQuery(pkg.label);
    setOpen(false);
  };

  return (
    <div className="relative w-72">
      <label className="mb-0.5 flex items-center gap-1 text-[10px] uppercase tracking-wider text-zinc-500">
        <span aria-hidden>⌕</span> find a district
      </label>
      <input
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
          setOpen(true);
        }}
        onFocus={() => setOpen(true)}
        onBlur={() => window.setTimeout(() => setOpen(false), 120)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && matches.length > 0) pick(matches[0]);
          if (event.key === "Escape") setOpen(false);
        }}
        placeholder="type a prefix — rgctl, crates, dashboard…"
        aria-label="Find a district by name"
        className="w-full rounded-md border border-sky-500/40 bg-black/80 px-2 py-1.5 text-[11px] text-zinc-100 shadow-[0_0_0_1px_rgba(56,189,248,0.15)] placeholder:text-zinc-500 focus:border-sky-400 focus:outline-none"
      />
      {open && (
        <ul className="absolute z-10 mt-1 max-h-56 w-full overflow-y-auto rounded-md border border-white/15 bg-[#0b1017] shadow-lg">
          {matches.length === 0 && (
            <li className="px-2 py-1 text-[10px] leading-snug text-zinc-500">
              no district label contains “{query.trim()}” — labels are directory paths, try `crates` or
              `dashboard`
            </li>
          )}
          {matches.map((pkg) => (
            <li key={pkg.id}>
              <button
                type="button"
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => pick(pkg)}
                className="flex w-full items-baseline justify-between gap-2 px-2 py-1 text-left text-[11px] text-zinc-300 hover:bg-white/[0.08]"
              >
                <span className="truncate font-mono">{pkg.label}</span>
                <span className="shrink-0 text-zinc-500">{pkg.size.toLocaleString()}m</span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/**
 * The symbol a level opened, read beside it.
 *
 * Double-clicking a member used to leave the room for Understand, which threw away the level the reader was
 * holding. The code opens here instead, at the width of a column; the control that matters is **maximize**,
 * because a function's source and its control-flow graph are two views of one thing and want reading together —
 * maximized is exactly the CFG lens's own framing, drawn by the same `SymbolReader`.
 */
function SymbolDrawer({
  entry,
  maximized,
  onToggleMaximize,
  onClose,
  onOpenInRoom,
}: {
  entry: FunctionEntry;
  maximized: boolean;
  onToggleMaximize: () => void;
  onClose: () => void;
  /** Present when the shell can carry a Focus to another room; absent, the control is not offered. */
  onOpenInRoom?: () => void;
}) {
  return (
    <div
      data-testid="symbol-drawer"
      className={`flex min-h-0 flex-col rounded-lg border border-white/10 bg-[#0a0e14] ${
        maximized ? "min-w-0 flex-1" : "w-[42%] min-w-[320px] max-w-[560px]"
      }`}
    >
      <div className="flex shrink-0 items-center gap-2 border-b border-white/8 px-2 py-1.5">
        <span className="truncate text-[12px] font-medium text-zinc-100">{entry.name}</span>
        <span className="truncate text-[10px] text-zinc-500">
          {entry.file_path?.split("/").pop() ?? "file unknown"}
        </span>
        <span className="ml-auto flex shrink-0 items-center gap-1">
          {onOpenInRoom && (
            <button
              type="button"
              onClick={onOpenInRoom}
              title="Carry this symbol to Understand as the Focus"
              className="rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-zinc-400 hover:border-white/20 hover:text-zinc-200"
            >
              open in Understand
            </button>
          )}
          <button
            type="button"
            onClick={onToggleMaximize}
            aria-pressed={maximized}
            title={maximized ? "Back to the level" : "Source beside the control-flow graph, full width"}
            className={`rounded border px-1.5 py-0.5 text-[10px] ${
              maximized
                ? "border-sky-400/40 bg-sky-400/10 text-sky-200"
                : "border-white/10 text-zinc-400 hover:border-white/20 hover:text-zinc-200"
            }`}
          >
            {maximized ? "restore" : "maximize"}
          </button>
          <button
            type="button"
            onClick={onClose}
            title="Close this symbol"
            className="rounded border border-white/10 px-1.5 py-0.5 text-[10px] text-zinc-400 hover:border-white/20 hover:text-zinc-200"
          >
            close
          </button>
        </span>
      </div>
      <div className="flex min-h-0 flex-1 flex-col overflow-y-auto p-2">
        <SymbolReader
          entry={entry}
          variant={maximized ? "source-and-graph" : "source"}
          sourceHeightClass="max-h-[62vh]"
        />
      </div>
    </div>
  );
}

/**
 * The level's own index: what is inside the district that is open.
 *
 * The right column answers "what am I looking at", so behind an open level the universe's district list is the
 * wrong answer — it lists 256 places, one of which the reader is already inside. This lists the members that were
 * *drawn*, states how many that is, and lets a reader find one by name instead of by sweeping a pointer across a
 * 3D field.
 */
function Members({
  district,
  nodes,
  drawn,
  selectedIndex,
  onPick,
  onOpen,
}: {
  district: string;
  nodes: MemberNode[];
  drawn: number;
  selectedIndex: number | null;
  onPick: (node: MemberNode) => void;
  onOpen: (node: MemberNode) => void;
}) {
  const [query, setQuery] = useState("");
  /** Rows rendered at once. Stated when it binds, like every other ceiling in this room. */
  const MAX_ROWS = 200;
  const sorted = useMemo(() => [...nodes].sort((a, b) => a.name.localeCompare(b.name)), [nodes]);
  const needle = query.trim().toLowerCase();
  const matched = needle ? sorted.filter((n) => n.name.toLowerCase().includes(needle)) : sorted;
  const rows = matched.slice(0, MAX_ROWS);

  return (
    <Panel title="Members" subtitle={`${drawn.toLocaleString()} drawn from ${district}`}>
      <input
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="filter by name…"
        aria-label="Filter the level's members by name"
        className="mb-1.5 w-full rounded border border-white/10 bg-black/40 px-2 py-1 text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:border-sky-400/60 focus:outline-none"
      />
      <ul className="max-h-[46vh] overflow-y-auto">
        {rows.map((node) => {
          const selected = node.index === selectedIndex;
          return (
            <li key={node.index}>
              <button
                type="button"
                onClick={() => onPick(node)}
                onDoubleClick={() => onOpen(node)}
                title={`${node.kind}${node.filePath ? ` · ${node.filePath}` : ""} — click to mark it in the level, double-click to open its code`}
                className={`flex w-full items-baseline gap-2 rounded px-1.5 py-0.5 text-left ${
                  selected ? "bg-amber-400/12 text-amber-100" : "text-zinc-300 hover:bg-white/[0.05]"
                }`}
              >
                <span className="truncate font-mono text-[11px]">{node.name}</span>
                <span className="ml-auto shrink-0 truncate text-[9.5px] text-zinc-500">
                  {node.filePath?.split("/").pop() ?? node.kind}
                </span>
              </button>
            </li>
          );
        })}
        {rows.length === 0 && (
          <li className="px-1.5 py-1 text-[10.5px] leading-relaxed text-zinc-500">
            no drawn member of “{query.trim()}” — this filter covers the members the level drew, not the whole
            graph
          </li>
        )}
      </ul>
      {matched.length > MAX_ROWS && (
        <p className="mt-1 text-[10px] leading-relaxed text-zinc-500">
          showing the first {MAX_ROWS} of {matched.length.toLocaleString()} matching members, by name — filter to
          narrow. The rest are drawn in the level.
        </p>
      )}
    </Panel>
  );
}

/**
 * Find a district by name.
 *
 * The room draws 256 boxes with no labels, so an instruction like "open `dashboard.slice`" was impossible to
 * act on — the only identification was a tooltip you found by sweeping the pointer across the scene. This list
 * names them, filters as you type, and **points the camera** at the one you choose, selecting it so Expand is
 * armed. Busiest districts first, because that is usually the one being looked for.
 *
 * It is not a symbol search: a district is a package, and the Focus palette (⌘K) is for the symbols inside one.
 */
function Districts({
  view,
  selectedId,
  expandedId,
  onPick,
}: {
  view: ArchitectureView;
  selectedId: number | null;
  expandedId: number | null;
  onPick: (pkg: PackageNode) => void;
}) {
  const [query, setQuery] = useState("");
  const all = useMemo(
    () => [...view.packages].sort((a, b) => b.size - a.size || a.label.localeCompare(b.label)),
    [view.packages],
  );
  const needle = query.trim().toLowerCase();
  const matches = needle ? all.filter((p) => p.label.toLowerCase().includes(needle)) : all;
  const shown = matches.slice(0, 12);

  return (
    <Panel title="Districts" subtitle={`${all.length} packages — click one to fly to it`}>
      <input
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="find a district by name…"
        aria-label="Find a district by name"
        className="mb-1.5 w-full rounded border border-white/10 bg-white/[0.04] px-2 py-1 text-[11px] text-zinc-200 placeholder:text-zinc-600 focus:border-sky-500/40 focus:outline-none"
      />
      {shown.length === 0 && (
        <p className="text-[11px] text-zinc-500">
          No district label contains “{query.trim()}”. Labels are directory paths as the export produced them,
          so try a prefix like `crates.` or `dashboard`.
        </p>
      )}
      <ul className="flex flex-col gap-0.5 text-[11px]">
        {shown.map((pkg) => {
          const isSelected = pkg.id === selectedId;
          const isExpanded = pkg.id === expandedId;
          return (
            <li key={pkg.id}>
              <button
                type="button"
                onClick={() => onPick(pkg)}
                className={`flex w-full items-baseline justify-between gap-2 rounded px-1 py-0.5 text-left ${
                  isSelected ? "bg-sky-500/15 text-sky-100" : "text-zinc-300 hover:bg-white/[0.06]"
                }`}
              >
                <span className="truncate font-mono">{pkg.label}</span>
                <span className="shrink-0 text-zinc-500">
                  {isExpanded ? "open · " : ""}
                  {pkg.size.toLocaleString()}m
                </span>
              </button>
            </li>
          );
        })}
      </ul>
      {matches.length > shown.length && (
        <p className="mt-1 text-[10px] text-zinc-600">
          {matches.length - shown.length} more match — keep typing to narrow.
        </p>
      )}
    </Panel>
  );
}

function Layers({ view }: { view: ArchitectureView }) {
  const report = channelReport(view);
  const shared = view.communities.filter((c) => c.shared).length;
  return (
    <Panel title="What is drawn" subtitle="three objects, three words">
      <ul className="flex flex-col gap-1.5 text-[11px] leading-relaxed text-zinc-400">
        <li>
          <b className="text-sky-300">namespace volume</b> — derived here from the package labels;{" "}
          {view.volumes.length} of them, holding {report.packagesInVolumes} of {view.disagreement.packages} metagraph
          nodes. Drawn only where a group holds at least two packages.
        </li>
        <li>
          <b className="text-zinc-200">package</b> — what the export calls a metagraph node: a directory. Height
          is members on a <b>log</b> scale; footprint is functions, sqrt-compressed and clamped to local spacing.
        </li>
        <li>
          <b className="text-emerald-300">community</b> — an overlay, never a container: {view.communities.length}{" "}
          of them, and only the <b>{shared}</b> that hold more than one node get a hue. The rest are neutral.
        </li>
      </ul>
      <p className="mt-2 text-[10.5px] leading-relaxed text-zinc-500">
        A package's community is a majority vote of its members and the vote counts are not exported, so nothing
        here places a package inside a community's boundary. {" "}
        {report.singletonNamespaces} packages are their own namespace and are not wrapped in a volume.
      </p>
    </Panel>
  );
}

function Namespaces({ view }: { view: ArchitectureView }) {
  const top = view.volumes.slice(0, 8);
  return (
    <Panel title="Namespace volumes" subtitle={`${view.volumes.length} derived groups`}>
      <ul className="flex flex-col gap-1 text-[11px]">
        {top.map((volume) => (
          <li key={volume.key} className="flex items-baseline justify-between gap-2">
            <span className="truncate text-zinc-300">{volume.key}</span>
            <span className="shrink-0 text-zinc-500">
              {volume.packages.length}p · {volume.members.toLocaleString()}m
            </span>
          </li>
        ))}
        {view.volumes.length > top.length && (
          <li className="text-[10.5px] text-zinc-600">and {view.volumes.length - top.length} more</li>
        )}
      </ul>
    </Panel>
  );
}

function Communities({
  view,
  sharedOnly,
  onToggleShared,
}: {
  view: ArchitectureView;
  sharedOnly: boolean;
  onToggleShared: () => void;
}) {
  const list = sharedOnly ? view.communities.filter((c) => c.shared) : view.communities;
  const top = list.slice(0, 8);
  const sharedCount = view.communities.filter((c) => c.shared).length;
  return (
    <Panel
      title="Communities"
      subtitle={
        view.modularity === null
          ? `${view.communities.length} · modularity not exported`
          : `${view.communities.length} · Fast-Leiden, modularity ${view.modularity.toFixed(3)}`
      }
      right={
        <button
          type="button"
          onClick={onToggleShared}
          aria-pressed={sharedOnly}
          className={`rounded border px-1.5 py-0.5 text-[10px] leading-none ${
            sharedOnly
              ? "border-emerald-500/40 bg-emerald-500/12 text-emerald-200"
              : "border-white/10 text-zinc-400 hover:text-zinc-200"
          }`}
        >
          {sharedOnly ? `shared only (${sharedCount})` : `all (${view.communities.length})`}
        </button>
      }
    >
      <ul className="flex flex-col gap-1 text-[11px]">
        {top.map((group) => (
          <li key={group.id} className="flex items-baseline justify-between gap-2">
            <span className={`truncate ${group.shared ? "text-emerald-300" : "text-zinc-400"}`}>{group.label}</span>
            <span className="shrink-0 text-zinc-500">
              {group.packageCount}n · {group.members.toLocaleString()}m
            </span>
          </li>
        ))}
        {list.length > top.length && (
          <li className="text-[10.5px] text-zinc-600">and {list.length - top.length} more</li>
        )}
      </ul>
      <p className="mt-2 text-[10.5px] leading-relaxed text-zinc-500">
        Labels are the clustering's own strings — sometimes a Java modifier, sometimes a path. They are shown
        verbatim: rewriting them would hide a producer defect and invent a naming scheme of our own.
      </p>
    </Panel>
  );
}

/** The buckets, named as buckets. Excluding them would hide a sixth of the code. */
function Buckets({ view }: { view: ArchitectureView }) {
  const covered = view.packages.reduce((n, p) => n + p.size, 0);
  const all = [...(view.remainder ? [view.remainder] : []), ...view.artifacts];
  if (all.length === 0) return null;
  const remainder = view.remainder;
  return (
    <Panel title="Not architecture" subtitle="buckets the export produced">
      <ul className="flex flex-col gap-1 text-[11px]">
        {all.map((bucket) => (
          <li key={bucket.id} className="flex items-baseline justify-between gap-2">
            <span className="truncate text-amber-200/80">{bucket.label}</span>
            <span className="shrink-0 text-zinc-500">
              {bucket.size.toLocaleString()}m · {bucket.functions.toLocaleString()}f
            </span>
          </li>
        ))}
      </ul>
      <p className="mt-2 text-[10.5px] leading-relaxed text-zinc-500">
        Edges are drawn at a **visibility floor**: brightness still orders them by `log(weight)`, but the
        weakest is lifted enough to see, so the web is a shape rather than a rumour.{" "}
        {remainder && (
          <>
            <code className="text-amber-200/80">(other)</code> is every package below the export's node cap —{" "}
            {remainder.size.toLocaleString()} members,{" "}
            {((remainder.size / Math.max(1, covered + remainder.size)) * 100).toFixed(1)}% of what this map covers —
            and its internal edges were never written, so it is drawn as a walled remainder.{" "}
          </>
        )}
        <b className="text-zinc-400">{view.artifactEdges.toLocaleString()}</b> of {view.edges.toLocaleString()} edges
        touch a bucket and are <b>not</b> drawn: the heaviest edge in the export joins two of them (65,216 — 3.4× the
        heaviest edge between two real packages), and an aggregate of unrelated files is not coupling.
      </p>
    </Panel>
  );
}
