import { useEffect, useRef, useState } from "preact/hooks";
import {
  EditorState,
  Compartment,
  StateField,
  RangeSetBuilder,
} from "@codemirror/state";
import {
  EditorView,
  lineNumbers,
  highlightActiveLineGutter,
  Decoration,
  type DecorationSet,
} from "@codemirror/view";
import { java } from "@codemirror/lang-java";
import { basicSetup } from "codemirror";
import { bundleDataUrl } from "./bundleUrl";
import { excerptSource, resolveSliceSource } from "./sourceResolver";
import { FunctionListLayout, FunctionListSidebar } from "./FunctionListSidebar";
import { shortPath, sliceEntryToListItem } from "./functionListUtils";
import { ViewLegend } from "./ViewLegend";
import { SLICE_LEGEND } from "./viewLegendData";
import type {
  SliceBundlePayload,
  SliceDirection,
  SliceIndexPayload,
  SliceResultPayload,
} from "./types";

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
          <SourceEditor
            source={sourceText}
            filePath={bundle.file_path}
            highlightLines={result?.lines ?? []}
            criterionLine={result?.criterion.line}
          />
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

function SourceEditor({
  source,
  filePath,
  highlightLines,
  criterionLine,
}: {
  source: string;
  filePath?: string | null;
  highlightLines: number[];
  criterionLine?: number;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const highlightCompartment = useRef(new Compartment());

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const highlightExt = buildLineHighlights(highlightLines, criterionLine);
    const state = EditorState.create({
      doc: source,
      extensions: [
        basicSetup,
        lineNumbers(),
        highlightActiveLineGutter(),
        java(),
        EditorView.editable.of(false),
        EditorState.readOnly.of(true),
        highlightCompartment.current.of(highlightExt),
      ],
    });

    const view = new EditorView({ state, parent: host });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
  }, [source]);

  useEffect(() => {
    const view = viewRef.current;
    if (!view) return;
    view.dispatch({
      effects: highlightCompartment.current.reconfigure(
        buildLineHighlights(highlightLines, criterionLine),
      ),
    });
  }, [highlightLines, criterionLine]);

  return (
    <div class="slice-editor-panel border rounded bg-white h-100 min-h-0 d-flex flex-column">
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">
        Source
        {filePath && <span class="text-muted fw-normal ms-2">{shortPath(filePath)}</span>}
      </div>
      <div ref={hostRef} class="slice-editor-host flex-grow-1 min-h-0 overflow-auto" />
      {highlightLines.length > 0 && (
        <ViewLegend hint="Source highlights" items={SLICE_LEGEND} />
      )}
    </div>
  );
}

function buildLineHighlights(lines: number[], criterionLine?: number) {
  const field = StateField.define<DecorationSet>({
    create(state) {
      return lineDecorations(state.doc, lines, criterionLine);
    },
    update(_deco, tr) {
      return lineDecorations(tr.state.doc, lines, criterionLine);
    },
    provide: (f) => EditorView.decorations.from(f),
  });
  return field;
}

function lineDecorations(
  doc: EditorState["doc"],
  lines: number[],
  criterionLine?: number,
): DecorationSet {
  const builder = new RangeSetBuilder<Decoration>();
  for (const lineNo of lines) {
    if (lineNo < 1 || lineNo > doc.lines) continue;
    const line = doc.line(lineNo);
    builder.add(
      line.from,
      line.from,
      Decoration.line({
        class:
          lineNo === criterionLine ? "cm-slice-criterion" : "cm-slice-line",
      }),
    );
  }
  return builder.finish();
}
