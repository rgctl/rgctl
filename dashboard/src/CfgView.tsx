import { useEffect, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import { bundleDataUrl } from "./bundleUrl";
import { mountSigmaWhenReady } from "./sigmaMount";
import type { CfgDetailPayload, CfgFunctionEntry, CfgIndexPayload } from "./types";

const EDGE_COLORS: Record<string, string> = {
  next: "#6c757d",
  if_true: "#198754",
  if_false: "#dc3545",
  jump: "#fd7e14",
  return: "#0d6efd",
  exception: "#6f42c1",
};

export function CfgView() {
  const [index, setIndex] = useState<CfgIndexPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<CfgDetailPayload | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("cfg_index.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`cfg_index.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: CfgIndexPayload) => {
        if (!cancelled) setIndex(data);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!selectedId) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    fetch(bundleDataUrl(`cfg/${selectedId}.json`))
      .then((r) => {
        if (!r.ok) throw new Error(`cfg detail HTTP ${r.status}`);
        return r.json();
      })
      .then((data: CfgDetailPayload) => {
        if (!cancelled) setDetail(data);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingDetail(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  if (error) {
    return <div class="alert alert-danger py-2 small mb-0">{error}</div>;
  }

  if (!index) {
    return <p class="text-muted mb-0">Loading CFG index…</p>;
  }

  if (!index.available) {
    return (
      <div>
        <h2 class="h5 mb-2">CFG / Dominance</h2>
        <p class="text-muted mb-2">
          No CFG archive in this bundle. Run discover with CFG analysis enabled:
        </p>
        <pre class="bg-light border rounded p-3 small mb-0">
          rbuilder discover . --languages java --cfg
        </pre>
        <p class="text-muted small mt-2 mb-0">
          Previews are exported from <code>cfg_pdg.archive.bin</code> into{" "}
          <code>cfg_index.json</code> and per-function JSON under <code>cfg/</code>.
        </p>
      </div>
    );
  }

  const filtered = filterFunctions(index.functions, search);

  return (
    <div class="cfg-view d-flex flex-column h-100 min-h-0">
      <div class="d-flex flex-wrap align-items-end gap-2 flex-shrink-0">
        <div class="flex-grow-1">
          <label class="form-label small mb-1" for="cfg-search">
            Function ({index.function_count} with CFG)
          </label>
          <input
            id="cfg-search"
            type="search"
            class="form-control form-control-sm"
            placeholder="Filter by name or path…"
            value={search}
            onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
          />
        </div>
        <select
          class="form-select form-select-sm"
          style={{ minWidth: "280px", maxWidth: "100%" }}
          value={selectedId ?? ""}
          onChange={(e) => {
            const v = (e.target as HTMLSelectElement).value;
            setSelectedId(v || null);
          }}
        >
          <option value="">Select function…</option>
          {filtered.map((f) => (
            <option key={f.function_id} value={f.function_id}>
              {f.name}
              {f.file_path ? ` — ${shortPath(f.file_path)}` : ""} ({f.block_count} blocks)
            </option>
          ))}
        </select>
      </div>

      {loadingDetail && <p class="text-muted small mb-0">Loading CFG…</p>}

      {detail && !loadingDetail && (
        <div class="cfg-detail d-flex flex-column flex-lg-row gap-3 flex-grow-1 min-h-0">
          <div class="cfg-graph-col flex-grow-1 min-h-0 d-flex flex-column">
            <CfgGraph detail={detail} />
          </div>
          <div class="cfg-dom-col min-h-0 d-flex flex-column">
            <DominancePanel detail={detail} />
          </div>
        </div>
      )}

      {!selectedId && (
        <p class="text-muted small mb-0">
          Pick a function to render its control-flow graph and dominator tree.
        </p>
      )}
    </div>
  );
}

function CfgGraph({ detail }: { detail: CfgDetailPayload }) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;

    return mountSigmaWhenReady(el, () => {
      sigmaRef.current?.kill();
      sigmaRef.current = null;

      const g = new Graph();
      const positions = layoutCfg(detail);

      for (const block of detail.blocks) {
        const isEntry = block.id === detail.entry;
        const isExit = detail.exits.includes(block.id);
        g.addNode(String(block.id), {
          label: block.label,
          x: positions[block.id]?.x ?? 0,
          y: positions[block.id]?.y ?? 0,
          size: isEntry || isExit ? 14 : 10,
          color: isEntry ? "#198754" : isExit ? "#dc3545" : "#0d6efd",
        });
      }

      for (const edge of detail.edges) {
        const key = `${edge.from}->${edge.to}:${edge.edge_type}`;
        if (!g.hasEdge(key)) {
          g.addEdgeWithKey(key, String(edge.from), String(edge.to), {
            color: EDGE_COLORS[edge.edge_type] ?? "#adb5bd",
            size: 2,
          });
        }
      }

      const sigma = new Sigma(g, el, {
        renderEdgeLabels: false,
        labelSize: 11,
        labelWeight: "500",
        defaultEdgeColor: "#adb5bd",
        minCameraRatio: 0.08,
        maxCameraRatio: 10,
      });
      sigmaRef.current = sigma;
      sigma.getCamera().animatedReset({ duration: 0 });

      const ro = new ResizeObserver(() => sigma.refresh());
      ro.observe(el);

      return () => {
        ro.disconnect();
        sigma.kill();
        if (sigmaRef.current === sigma) {
          sigmaRef.current = null;
        }
      };
    });
  }, [detail]);

  return (
    <div class="cfg-graph-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">
        CFG — {detail.name}
        {detail.file_path && (
          <span class="text-muted fw-normal ms-2">{shortPath(detail.file_path)}</span>
        )}
      </div>
      <div class="cfg-graph-wrap flex-grow-1 min-h-0">
        <div ref={containerRef} class="sigma-host" />
      </div>
      <div class="border-top py-1 px-3 small d-flex flex-wrap gap-2 flex-shrink-0">
        {Object.entries(EDGE_COLORS).map(([k, c]) => (
          <span key={k} class="d-inline-flex align-items-center gap-1">
            <span
              style={{
                width: "10px",
                height: "10px",
                background: c,
                display: "inline-block",
                borderRadius: "2px",
              }}
            />
            {k.replace("_", " ")}
          </span>
        ))}
      </div>
    </div>
  );
}

function DominancePanel({ detail }: { detail: CfgDetailPayload }) {
  const selectedBlock = detail.entry;

  return (
    <div class="cfg-dom-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">Dominance</div>
      <div class="flex-grow-1 min-h-0 overflow-auto small">
        <table class="table table-sm table-striped mb-0">
          <thead>
            <tr>
              <th>Block</th>
              <th>idom</th>
              <th>Frontier</th>
            </tr>
          </thead>
          <tbody>
            {detail.blocks.map((b) => (
              <tr key={b.id} class={b.id === selectedBlock ? "table-success" : ""}>
                <td>
                  <code>{b.label}</code>
                </td>
                <td>{detail.idom[b.id]?.toString() ?? "—"}</td>
                <td>{(detail.dominance_frontiers[b.id] ?? []).join(", ") || "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {detail.blocks.some((b) => b.statements.length > 0) && (
          <div class="p-2 border-top">
            <div class="fw-semibold mb-1">Entry block preview</div>
            <pre class="bg-light rounded p-2 mb-0" style={{ fontSize: "0.75rem" }}>
              {detail.blocks.find((b) => b.id === detail.entry)?.statements.join("\n") ??
                "(empty)"}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

function layoutCfg(detail: CfgDetailPayload): Record<number, { x: number; y: number }> {
  const layers = new Map<number, number>();
  const adj = new Map<number, number[]>();
  for (const e of detail.edges) {
    if (!adj.has(e.from)) adj.set(e.from, []);
    adj.get(e.from)!.push(e.to);
  }

  const queue: number[] = [detail.entry];
  const depth = new Map<number, number>();
  depth.set(detail.entry, 0);

  while (queue.length) {
    const n = queue.shift()!;
    const d = depth.get(n)!;
    for (const next of adj.get(n) ?? []) {
      if (!depth.has(next)) {
        depth.set(next, d + 1);
        queue.push(next);
      }
    }
  }

  for (const b of detail.blocks) {
    const d = depth.get(b.id) ?? 0;
    const y = layers.get(d) ?? 0;
    layers.set(d, y + 1);
  }

  const layerCounts = new Map<number, number>();
  const out: Record<number, { x: number; y: number }> = {};

  for (const b of detail.blocks) {
    const d = depth.get(b.id) ?? 0;
    const idx = layerCounts.get(d) ?? 0;
    layerCounts.set(d, idx + 1);
    const total = layers.get(d) ?? 1;
    out[b.id] = {
      x: d * 120,
      y: (idx - (total - 1) / 2) * 80,
    };
  }

  return out;
}

function filterFunctions(list: CfgFunctionEntry[], search: string): CfgFunctionEntry[] {
  const q = search.trim().toLowerCase();
  if (!q) return list;
  return list.filter(
    (f) =>
      f.name.toLowerCase().includes(q) ||
      (f.file_path?.toLowerCase().includes(q) ?? false),
  );
}

function shortPath(p: string): string {
  const parts = p.split(/[/\\]/);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : p;
}
