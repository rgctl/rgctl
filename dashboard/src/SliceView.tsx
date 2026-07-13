import { useEffect, useState } from "preact/hooks";
import { lazy, Suspense } from "preact/compat";
import { bundleDataUrl } from "./bundleUrl";
import { excerptSource, resolveSliceSource } from "./sourceResolver";
import { FunctionListLayout, FunctionListSidebar } from "./FunctionListSidebar";
import { sliceEntryToListItem } from "./functionListUtils";

import type {
  SliceBundlePayload,
  SliceDirection,
  SliceIndexPayload,
  SliceResultPayload,
} from "./types";

const SliceSourceEditor = lazy(() =>
  import("./sliceEditor").then((m) => ({ default: m.SliceSourceEditor })),
);

export interface SliceViewProps {
  computeSlice: (
    functionId: string,
    line: number,
    variable: string,
    direction: SliceDirection,
  ) => Promise<SliceResultPayload>;
}

export function SliceView({ computeSlice }: SliceViewProps) {
  const [index, setIndex] = useState<SliceIndexPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<SliceBundlePayload | null>(null);
  const [sourceText, setSourceText] = useState("");
  const [line, setLine] = useState(1);
  const [variable, setVariable] = useState("");
  const [direction, setDirection] = useState<SliceDirection>("backward");
  const [result, setResult] = useState<SliceResultPayload | null>(null);
  const [computing, setComputing] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("slice_index.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`slice_index.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: SliceIndexPayload) => {
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
      setBundle(null);
      setSourceText("");
      setResult(null);
      return;
    }
    let cancelled = false;
    fetch(bundleDataUrl(`slice/${selectedId}.json`))
      .then((r) => {
        if (!r.ok) throw new Error(`slice bundle HTTP ${r.status}`);
        return r.json();
      })
      .then(async (data: SliceBundlePayload) => {
        if (cancelled) return;
        setBundle(data);
        const resolved = data.source
          ? data.source
          : excerptSource(
              await resolveSliceSource(data),
              data.start_line,
              data.end_line,
            );
        if (!cancelled) {
          setSourceText(resolved);
          setLine(1);
          setVariable("");
          setResult(null);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId]);

  const runSlice = async () => {
    if (!selectedId || !variable.trim()) return;
    setComputing(true);
    setError(null);
    try {
      const payload = await computeSlice(
        selectedId,
        line,
        variable.trim(),
        direction,
      );
      setResult(payload);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setComputing(false);
    }
  };

  if (error && !index) {
    return <div class="alert alert-danger py-2 small mb-0">{error}</div>;
  }

  if (!index) {
    return <p class="text-muted mb-0">Loading slice index…</p>;
  }

  if (!index.available) {
    return (
      <div>
        <h2 class="h5 mb-2">Program Slicing</h2>
        <p class="text-muted mb-2">
          Slice bundles require CFG/PDG analysis. Run discover with{" "}
          <code>--cfg</code>:
        </p>
        <pre class="bg-light border rounded p-3 small mb-0">
          rbuilder discover . --languages java --cfg
        </pre>
      </div>
    );
  }

  return (
    <FunctionListLayout
      sidebar={
        <FunctionListSidebar
          count={index.function_count}
          items={index.functions.map(sliceEntryToListItem)}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />
      }
    >
      <div class="slice-view d-flex flex-column h-100 min-h-0 gap-3 p-3">
        <div class="d-flex flex-wrap align-items-end gap-2 flex-shrink-0">
        <div>
          <label class="form-label small mb-1" for="slice-line">
            Line
          </label>
          <input
            id="slice-line"
            type="number"
            min={1}
            class="form-control form-control-sm"
            style="width: 5rem"
            value={line}
            onInput={(e) => setLine(Number((e.target as HTMLInputElement).value))}
          />
        </div>
        <div class="flex-grow-1">
          <label class="form-label small mb-1" for="slice-var">
            Variable
          </label>
          <input
            id="slice-var"
            type="text"
            class="form-control form-control-sm"
            placeholder="e.g. orderId"
            value={variable}
            onInput={(e) => setVariable((e.target as HTMLInputElement).value)}
          />
        </div>
        <div>
          <label class="form-label small mb-1" for="slice-dir">
            Direction
          </label>
          <select
            id="slice-dir"
            class="form-select form-select-sm"
            value={direction}
            onChange={(e) =>
              setDirection((e.target as HTMLSelectElement).value as SliceDirection)
            }
          >
            <option value="backward">Backward</option>
            <option value="forward">Forward</option>
          </select>
        </div>
        <button
          type="button"
          class="btn btn-primary btn-sm"
          disabled={!selectedId || !variable.trim() || computing}
          onClick={() => void runSlice()}
        >
          {computing ? "Slicing…" : "Compute slice"}
        </button>
      </div>

      {error && (
        <div class="alert alert-warning py-2 small mb-0 flex-shrink-0">{error}</div>
      )}

      {result && (
        <div class="small text-muted flex-shrink-0">
          {result.direction} slice: {result.lines.length} lines ·{" "}
          {result.reduction_percent.toFixed(1)}% reduction · {result.nodes.length}{" "}
          PDG nodes
        </div>
      )}

      {bundle && (
        <div class="flex-grow-1 min-h-0">
          <Suspense fallback={<p class="text-muted small p-3 mb-0">Loading editor…</p>}>
            <SliceSourceEditor
              source={sourceText}
              filePath={bundle.file_path}
              highlightLines={result?.lines ?? []}
              criterionLine={result?.criterion.line}
            />
          </Suspense>
        </div>
      )}

      {!selectedId && (
        <p class="text-muted small mb-0">
          Select a function, set line + variable, then compute a slice.
        </p>
      )}
      </div>
    </FunctionListLayout>
  );
}
