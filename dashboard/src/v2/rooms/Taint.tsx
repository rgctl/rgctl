/**
 * Taint room — where does untrusted data reach a sink?
 *
 * Layout:
 *   center = canvas (path by default; CFG / DFG / blast when chosen from Context)
 *   table  = triage with severity + source/sink tags (v1 parity)
 *   side   = filters + clickable CFG / DFG / blast that swap the canvas
 */
import { useMemo, useState, type ReactNode } from "react";
import { useFocus } from "../useFocus";
import { useCfgDetail } from "../useCfgDetail";
import { useDfgDetail } from "../hooks/useDfgDetail";
import { useBlastRadius } from "../hooks/useBlastRadius";
import { useTaintRoom, type TaintFilters } from "../hooks/useTaintRoom";
import type { TaintFlowView, TaintFunctionEntry } from "../data/loadTaint";
import { Chip, Panel, Unavailable } from "../shell";
import { TaintPathStage } from "./TaintPathStage";
import { CfgGraph, maxDominatorDepth } from "./CfgGraph";
import { DfgGraph } from "./DfgGraph";
import { BlastStage } from "./BlastStage";

const DISCOVER_HINT = "`rgctl discover . --with-universe --with-taint`";

export type TaintCanvasMode = "path" | "cfg" | "dfg" | "blast";

/** Severity bands match v1 `severityBadge` / TAINT_SEVERITY_LEGEND. */
export function severityTone(severity: number): "rose" | "amber" | "blue" | "zinc" {
  if (severity >= 9) return "rose";
  if (severity >= 7) return "amber";
  if (severity >= 5) return "blue";
  return "zinc";
}

/** Readable flow identity — exports have no separate name field. */
export function flowDisplayName(flow: TaintFlowView): string {
  return `${flow.source_type} → ${flow.sink_type}`;
}

function SeverityTag({ severity }: { severity: number }) {
  const tone = severityTone(severity);
  const styles: Record<typeof tone, string> = {
    rose: "border-rose-500/40 bg-rose-500/15 text-rose-200",
    amber: "border-amber-500/40 bg-amber-500/15 text-amber-200",
    blue: "border-sky-500/40 bg-sky-500/15 text-sky-200",
    zinc: "border-white/15 bg-white/6 text-zinc-300",
  };
  return (
    <span
      className={`inline-flex min-w-[1.5rem] items-center justify-center rounded border px-1.5 py-0.5 font-mono text-[10px] tabular-nums ${styles[tone]}`}
      title={
        severity >= 9
          ? "Critical (9+)"
          : severity >= 7
            ? "High (7–8)"
            : severity >= 5
              ? "Medium (5–6)"
              : "Low (<5)"
      }
    >
      {severity}
    </span>
  );
}

export function TaintRoom() {
  const { focus, entry } = useFocus();
  const { view, filters, setFilters, setActiveFunctionId, setSelectedFlowId } = useTaintRoom(focus);
  const [canvasMode, setCanvasMode] = useState<TaintCanvasMode>("path");
  const [blastDepth, setBlastDepth] = useState(5);
  const [cfgBlock, setCfgBlock] = useState<number | null>(null);

  const activeId = view.kind === "ready" ? view.activeFunctionId : null;
  const cfg = useCfgDetail(activeId ?? "");
  const detailDir = view.kind === "ready" ? (view.dataflow?.detail_dir ?? "slice") : "slice";
  const dfg = useDfgDetail(activeId, canvasMode === "dfg", detailDir);
  const blastLive = useBlastRadius(activeId, blastDepth, canvasMode === "blast");

  if (view.kind === "loading") {
    return (
      <Panel title="Taint" subtitle="reading the bundle">
        <p className="text-[11px] text-zinc-500">Reading `taint_index.json`…</p>
      </Panel>
    );
  }

  if (view.kind === "missing" || view.kind === "unavailable") {
    return (
      <Panel title="Taint" subtitle="no taint analysis in this bundle">
        <Unavailable
          tag="not exported"
          what="This bundle carries no usable taint analysis"
          why={`Run ${DISCOVER_HINT} (CFG/PDG is implied). A bundle without that export shows this absence rather than an empty table.`}
        />
      </Panel>
    );
  }

  if (view.kind === "empty") {
    return (
      <Panel title="Taint" subtitle="analysis ran, nothing found">
        <Unavailable
          tag="no flows"
          what="Taint analysis ran but found no flows"
          why={`\`taint_index.json\` is present and empty — distinct from never running ${DISCOVER_HINT}.`}
        />
      </Panel>
    );
  }

  const activeFn = view.functions.find((f) => f.function_id === view.activeFunctionId);
  const seedName = activeFn?.name ?? entry?.name ?? "function";
  const blastScore = view.blastForActive.kind === "ready" ? view.blastForActive.score : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2" data-testid="taint-room">
      <Panel
        title="Taint"
        subtitle="untrusted data → sink"
        right={
          <span className="flex items-center gap-1.5">
            <Chip tone="amber">{view.index.vulnerable_flows} vulnerable</Chip>
            <Chip tone="zinc">{view.index.total_flows} flows</Chip>
          </span>
        }
      >
        <p className="text-[11.5px] leading-relaxed text-zinc-400">
          Select a flow in the table. Path is the default canvas; open{" "}
          <b className="text-zinc-200">CFG</b>, <b className="text-zinc-200">DFG</b>, or{" "}
          <b className="text-zinc-200">Blast</b> from Context to draw that analysis here. Analysis is{" "}
          <b className="text-zinc-200">intra-procedural</b>
          {focus && entry ? (
            <>
              {" "}
              · Focus <code className="text-sky-300">{entry.name}</code>
            </>
          ) : (
            <> · no Focus (all tainted functions)</>
          )}
          .
        </p>
      </Panel>

      {view.focusMiss && (
        <Unavailable
          tag="focus has no taint"
          what="This Focus has no taint flows in the bundle"
          why="Clear Focus or pick a symbol that appears in the flows table."
        />
      )}

      <div className="grid min-h-0 flex-1 gap-2 lg:grid-cols-[minmax(0,1fr)_280px]">
        <div className="flex min-h-0 flex-col gap-2">
          <CanvasChrome mode={canvasMode} onMode={setCanvasMode}>
            {canvasMode === "path" && <TaintPathStage flow={view.selectedFlow} />}
            {canvasMode === "cfg" && (
              <CfgCanvas cfg={cfg} selected={cfgBlock} onSelect={setCfgBlock} />
            )}
            {canvasMode === "dfg" && (
              <DfgCanvas
                dfg={dfg}
                variable={view.selectedFlow?.variable ?? null}
              />
            )}
            {canvasMode === "blast" && (
              <BlastStage
                seedName={seedName}
                depth={blastDepth}
                onDepthChange={setBlastDepth}
                score={blastScore}
                blast={blastLive}
              />
            )}
          </CanvasChrome>
          <FlowTable
            functions={view.functions}
            activeFunctionId={view.activeFunctionId}
            flows={view.flows}
            selectedFlowId={view.selectedFlowId}
            bundleLoading={view.bundleLoading}
            onSelectFunction={setActiveFunctionId}
            onSelectFlow={(id) => {
              setSelectedFlowId(id);
              setCanvasMode("path");
            }}
          />
        </div>

        <ContextPanel
          functionName={activeFn?.name ?? entry?.name ?? null}
          filters={filters}
          onFilters={setFilters}
          canvasMode={canvasMode}
          onCanvasMode={setCanvasMode}
          cfg={cfg}
          blast={view.blastForActive}
          blastLive={blastLive}
          dataflowEntry={view.dataflowForActive}
          dataflowAvailable={view.dataflow?.available === true}
        />
      </div>
    </div>
  );
}

function CanvasChrome({
  mode,
  onMode,
  children,
}: {
  mode: TaintCanvasMode;
  onMode: (m: TaintCanvasMode) => void;
  children: ReactNode;
}) {
  const tabs: { id: TaintCanvasMode; label: string }[] = [
    { id: "path", label: "Path" },
    { id: "cfg", label: "CFG" },
    { id: "dfg", label: "DFG" },
    { id: "blast", label: "Blast" },
  ];
  return (
    <Panel
      title="Canvas"
      subtitle={
        mode === "path"
          ? "taint path"
          : mode === "cfg"
            ? "control-flow graph"
            : mode === "dfg"
              ? "def-use / PDG"
              : "blast radius"
      }
      right={
        <div className="flex gap-0.5" data-testid="taint-canvas-modes">
          {tabs.map((t) => (
            <button
              key={t.id}
              type="button"
              onClick={() => onMode(t.id)}
              className={`rounded px-1.5 py-0.5 text-[10px] ${
                mode === t.id
                  ? "bg-sky-500/20 text-sky-200"
                  : "text-zinc-500 hover:text-zinc-300"
              }`}
            >
              {t.label}
            </button>
          ))}
        </div>
      }
    >
      <div className="min-h-[220px]">{children}</div>
    </Panel>
  );
}

function CfgCanvas({
  cfg,
  selected,
  onSelect,
}: {
  cfg: ReturnType<typeof useCfgDetail>;
  selected: number | null;
  onSelect: (id: number | null) => void;
}) {
  if (cfg.kind === "loading") {
    return <p className="text-[11px] text-zinc-500">Loading CFG…</p>;
  }
  if (cfg.kind !== "ready") {
    return (
      <Unavailable
        tag="no CFG"
        what="No control-flow graph for this function"
        why={cfg.kind === "absent" || cfg.kind === "failed" ? cfg.reason : "CFG unavailable."}
      />
    );
  }
  return (
    <div data-testid="taint-cfg-stage">
      <CfgGraph
        cfg={cfg.cfg}
        selected={selected}
        onSelect={onSelect}
        maxHeightClass="max-h-[48vh]"
        footnote={
          <span className="text-[10px] text-zinc-600">
            Dominator depth is vertical position · click a block to select
          </span>
        }
      />
    </div>
  );
}

function DfgCanvas({
  dfg,
  variable,
}: {
  dfg: ReturnType<typeof useDfgDetail>;
  variable: string | null;
}) {
  if (dfg.kind === "idle" || dfg.kind === "loading") {
    return <p className="text-[11px] text-zinc-500">Loading DFG…</p>;
  }
  if (dfg.kind !== "ready") {
    return (
      <Unavailable
        tag="no DFG"
        what="No dataflow / PDG detail for this function"
        why={dfg.reason}
      />
    );
  }
  return <DfgGraph dfg={dfg.dfg} highlightVariable={variable} maxHeightClass="max-h-[48vh]" />;
}

function shortFile(path?: string | null): string {
  if (!path) return "";
  const parts = path.split("/");
  return parts.slice(-2).join("/");
}

/**
 * Pick one tainted function — never a wrap-all chip wall (metasfresh has ~1k).
 * Search narrows the select; options sort by vulnerable/flow count.
 */
function FunctionPicker({
  functions,
  activeFunctionId,
  onSelectFunction,
}: {
  functions: TaintFunctionEntry[];
  activeFunctionId: string | null;
  onSelectFunction: (id: string) => void;
}) {
  const [query, setQuery] = useState("");
  const ranked = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const list = needle
      ? functions.filter(
          (f) =>
            f.name.toLowerCase().includes(needle) ||
            (f.file_path ?? "").toLowerCase().includes(needle),
        )
      : functions;
    return [...list].sort(
      (a, b) =>
        b.vulnerable_count - a.vulnerable_count ||
        b.flow_count - a.flow_count ||
        a.name.localeCompare(b.name),
    );
  }, [functions, query]);

  const active = functions.find((f) => f.function_id === activeFunctionId) ?? null;
  const options =
    active && !ranked.some((f) => f.function_id === active.function_id)
      ? [active, ...ranked]
      : ranked;

  return (
    <div className="mb-3 flex flex-col gap-1.5" data-testid="taint-function-picker">
      <div className="flex flex-wrap items-center gap-2">
        <label className="text-[10px] uppercase tracking-wider text-zinc-600" htmlFor="taint-fn-filter">
          Function
        </label>
        <input
          id="taint-fn-filter"
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Filter by name or path…"
          className="min-w-[12rem] flex-1 rounded border border-white/10 bg-black/30 px-2 py-1 font-mono text-[11px] text-zinc-200 placeholder:text-zinc-600"
        />
        <span className="text-[10px] tabular-nums text-zinc-500">
          {ranked.length}/{functions.length}
        </span>
      </div>
      <select
        aria-label="Tainted function"
        value={activeFunctionId ?? ""}
        onChange={(e) => onSelectFunction(e.target.value)}
        className="w-full rounded border border-white/10 bg-black/40 px-2 py-1.5 font-mono text-[11px] text-zinc-100"
        data-testid="taint-function-select"
      >
        {options.length === 0 ? (
          <option value="">No functions match</option>
        ) : (
          options.map((f) => (
            <option key={f.function_id} value={f.function_id}>
              {f.name}
              {f.file_path ? ` · ${shortFile(f.file_path)}` : ""}
              {` · ${f.vulnerable_count}v / ${f.flow_count} flows`}
            </option>
          ))
        )}
      </select>
      {active && (
        <p className="text-[10px] text-zinc-500">
          Showing flows in <code className="text-zinc-300">{active.name}</code>
          {active.file_path ? (
            <>
              {" "}
              · <span className="font-mono text-zinc-600">{shortFile(active.file_path)}</span>
            </>
          ) : null}
        </p>
      )}
    </div>
  );
}

function FlowTable({
  functions,
  activeFunctionId,
  flows,
  selectedFlowId,
  bundleLoading,
  onSelectFunction,
  onSelectFlow,
}: {
  functions: TaintFunctionEntry[];
  activeFunctionId: string | null;
  flows: TaintFlowView[];
  selectedFlowId: number | null;
  bundleLoading: boolean;
  onSelectFunction: (id: string) => void;
  onSelectFlow: (id: number) => void;
}) {
  return (
    <Panel
      title="Flows"
      subtitle={
        bundleLoading
          ? "loading…"
          : `${flows.length} flow${flows.length === 1 ? "" : "s"} in selected function — click a row`
      }
    >
      {functions.length > 1 && (
        <FunctionPicker
          functions={functions}
          activeFunctionId={activeFunctionId}
          onSelectFunction={onSelectFunction}
        />
      )}
      <div className="max-h-[28vh] overflow-auto">
        <table className="w-full border-collapse text-left text-[11px]" data-testid="taint-flows">
          <thead className="sticky top-0 bg-[#0c1017]">
            <tr className="text-[10px] uppercase tracking-wider text-zinc-600">
              <th className="px-1 py-1">flow</th>
              <th className="px-1 py-1">severity</th>
              <th className="px-1 py-1">status</th>
              <th className="px-1 py-1">var</th>
            </tr>
          </thead>
          <tbody>
            {flows.length === 0 ? (
              <tr>
                <td colSpan={4} className="px-1 py-2 text-zinc-500">
                  {bundleLoading
                    ? "Loading flows…"
                    : activeFunctionId
                      ? "No flows match the filters for this function."
                      : "Select a function to list its taint flows."}
                </td>
              </tr>
            ) : (
              flows.map((f) => (
                <tr
                  key={f.id}
                  onClick={() => onSelectFlow(f.id)}
                  className={`cursor-pointer border-t border-white/5 ${
                    selectedFlowId === f.id
                      ? "bg-sky-500/10 text-sky-100"
                      : "text-zinc-300 hover:bg-white/[0.03]"
                  }`}
                >
                  <td className="px-1 py-1.5 font-medium text-zinc-100">{flowDisplayName(f)}</td>
                  <td className="px-1 py-1.5">
                    <SeverityTag severity={f.severity} />
                  </td>
                  <td className="px-1 py-1.5">
                    {f.vulnerable ? (
                      <span className="rounded border border-rose-500/35 bg-rose-500/10 px-1.5 py-0.5 text-[10px] text-rose-200">
                        vulnerable
                      </span>
                    ) : (
                      <span className="rounded border border-emerald-500/35 bg-emerald-500/10 px-1.5 py-0.5 text-[10px] text-emerald-200">
                        sanitized
                      </span>
                    )}
                  </td>
                  <td className="px-1 py-1.5 font-mono">
                    <code>{f.variable}</code>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
      <p className="mt-2 text-[10px] text-zinc-600">
        Severity: critical 9+ · high 7–8 · medium 5–6 · low &lt;5
      </p>
    </Panel>
  );
}

/** One side panel — filters + canvas mode switches for CFG / DFG / blast. */
function ContextPanel({
  functionName,
  filters,
  onFilters,
  canvasMode,
  onCanvasMode,
  cfg,
  blast,
  blastLive,
  dataflowEntry,
  dataflowAvailable,
}: {
  functionName: string | null;
  filters: TaintFilters;
  onFilters: (f: TaintFilters) => void;
  canvasMode: TaintCanvasMode;
  onCanvasMode: (m: TaintCanvasMode) => void;
  cfg: ReturnType<typeof useCfgDetail>;
  blast: ReturnType<typeof import("../data/loadTaint").blastForFunction>;
  blastLive: ReturnType<typeof useBlastRadius>;
  dataflowEntry: { pdg_nodes?: number; data_edges?: number; block_count?: number } | null;
  dataflowAvailable: boolean;
}) {
  const cfgLine =
    cfg.kind === "ready"
      ? `${cfg.cfg.blocks.length} blocks · depth ${maxDominatorDepth(cfg.cfg)}`
      : cfg.kind === "loading"
        ? "…"
        : "not in bundle";

  const dfgLine = !dataflowAvailable
    ? "not exported"
    : dataflowEntry
      ? `${dataflowEntry.pdg_nodes ?? "—"} PDG · ${dataflowEntry.data_edges ?? "—"} data edges`
      : "no row for this function";

  const blastLine =
    blastLive.kind === "ready"
      ? `direct ${blastLive.payload.direct_caller_count} · zone ${blastLive.payload.impact_zone_count} · score ${blastLive.payload.score.toFixed(3)}`
      : blastLive.kind === "loading"
        ? "loading WASM engine…"
        : blastLive.kind === "failed"
          ? "engine failed"
          : blast.kind === "ready"
            ? `score ${blast.score.score.toFixed(3)} · direct ${blast.score.direct} · zone ${blast.score.zone}`
            : "open to expand (WASM)";

  return (
    <div className="flex min-h-0 flex-col gap-2 overflow-y-auto">
      <Panel
        title="Context"
        subtitle={functionName ? `function ${functionName}` : "no function selected"}
      >
        <label className="mb-2 flex items-center gap-2 text-[11px] text-zinc-400">
          <input
            type="checkbox"
            checked={filters.vulnerableOnly}
            onChange={(e) => onFilters({ ...filters, vulnerableOnly: e.target.checked })}
          />
          Vulnerable only
        </label>
        <label className="mb-3 flex items-center gap-2 text-[11px] text-zinc-400">
          <span className="w-[4.5rem] shrink-0">Min sev</span>
          <input
            type="range"
            min={0}
            max={10}
            step={1}
            value={filters.minSeverity}
            onChange={(e) => onFilters({ ...filters, minSeverity: Number(e.target.value) })}
            className="min-w-0 flex-1"
          />
          <span className="w-5 tabular-nums text-zinc-300">{filters.minSeverity}</span>
        </label>

        <div className="space-y-1.5" data-testid="taint-context-metrics">
          <ModeButton
            active={canvasMode === "path"}
            label="Path"
            detail="source → sink statements"
            onClick={() => onCanvasMode("path")}
          />
          <ModeButton
            active={canvasMode === "cfg"}
            label="CFG"
            detail={cfgLine}
            onClick={() => onCanvasMode("cfg")}
            testId="taint-open-cfg"
          />
          <ModeButton
            active={canvasMode === "dfg"}
            label="DFG"
            detail={dfgLine}
            onClick={() => onCanvasMode("dfg")}
            testId="taint-open-dfg"
          />
          <ModeButton
            active={canvasMode === "blast"}
            label="Blast"
            detail={blastLine}
            onClick={() => onCanvasMode("blast")}
            testId="taint-open-blast"
          />
        </div>

        <p className="mt-3 text-[10.5px] leading-relaxed text-zinc-500" data-testid="taint-intra-proc">
          Flows are <b className="text-zinc-400">intra-procedural</b> — they stop at the function
          boundary. Blast shells use caller depth on the canvas.
        </p>
      </Panel>
    </div>
  );
}

function ModeButton({
  active,
  label,
  detail,
  onClick,
  testId,
}: {
  active: boolean;
  label: string;
  detail: string;
  onClick: () => void;
  testId?: string;
}) {
  return (
    <button
      type="button"
      data-testid={testId}
      onClick={onClick}
      className={`w-full rounded border px-2 py-1.5 text-left transition-colors ${
        active
          ? "border-sky-500/40 bg-sky-500/12"
          : "border-white/10 bg-transparent hover:border-white/20 hover:bg-white/[0.03]"
      }`}
    >
      <div className="text-[10px] uppercase tracking-wider text-zinc-500">{label}</div>
      <div className={`mt-0.5 text-[11px] ${active ? "text-sky-100" : "text-zinc-300"}`}>{detail}</div>
    </button>
  );
}
