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
import { useEffect, useRef, useState, type ReactNode } from "react";
import { Panel, Unavailable, Chip } from "../shell";
import { channelReport, loadArchitecture, scalesFor } from "../architecture";
import type { ArchitectureState, ArchitectureView } from "../architectureTypes";
import type { HoverEntry, UniverseLayer, UniverseScene } from "../universe/UniverseScene";

type Loaded = { kind: "loading" } | ArchitectureState;

export function ArchitectureRoom() {
  const [state, setState] = useState<Loaded>({ kind: "loading" });
  const [layer, setLayer] = useState<UniverseLayer>("packages");
  const [edges, setEdges] = useState(true);
  const [sharedOnly, setSharedOnly] = useState(false);
  const [hover, setHover] = useState<HoverEntry | null>(null);
  const [sceneFailed, setSceneFailed] = useState<string | null>(null);

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
        setSceneFailed((error as Error).message);
        return;
      }
      if (disposed) return;
      if (!module.webglAvailable()) {
        setSceneFailed("this browser could not create a WebGL context");
        return;
      }
      try {
        scene = new module.UniverseScene(host);
      } catch (error) {
        setSceneFailed((error as Error).message);
        return;
      }
      scene.onHoverChange(setHover);
      scene.attachPointer(host);
      const scales = scalesFor(view);
      scene.setEdges(
        edgeListFor(view),
        new Map(view.packages.map((p) => [p.id, p])),
      );
      scene.update({ view, layer, edges, height: scales.height, footprint: scales.footprint });
      sceneRef.current = scene;
      observer = new ResizeObserver(() => scene?.resize(host));
      observer.observe(host);
    })();

    return () => {
      disposed = true;
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
      <div className="grid min-h-0 flex-1 gap-2 md:grid-cols-[1fr_300px]">
        <div className="relative min-h-[280px] overflow-hidden rounded-lg border border-white/8 bg-[#060a12]">
          <div ref={hostRef} className="absolute inset-0" data-testid="universe-canvas" />
          {sceneFailed && (
            <div className="absolute inset-0 flex items-center justify-center p-4">
              <Unavailable
                tag="drawing unavailable"
                what="The drawing could not be started"
                why={`${sceneFailed}. The coverage, the layers and every list below are unaffected — a failure of the canvas is not a failure of the room.`}
              />
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
          <div className="absolute bottom-2 right-2 flex gap-1">
            <Toggle on={layer === "packages"} onClick={() => setLayer("packages")}>namespace › package</Toggle>
            <Toggle on={layer === "communities"} onClick={() => setLayer("communities")}>communities</Toggle>
            <Toggle on={edges} onClick={() => setEdges((value) => !value)}>edges {edges ? "on" : "off"}</Toggle>
          </div>
        </div>
        <div className="flex min-h-0 flex-col gap-2 overflow-y-auto">
          <Layers view={view!} />
          <Namespaces view={view!} />
          <Communities
            view={view!}
            sharedOnly={sharedOnly}
            onToggleShared={() => setSharedOnly((value) => !value)}
          />
          <Buckets view={view!} />
        </div>
      </div>
    </div>
  );
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
