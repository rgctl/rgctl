import { useEffect, useRef } from "preact/hooks";
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
import { shortPath } from "./functionListUtils";
import { ViewLegend } from "./ViewLegend";
import { SLICE_LEGEND } from "./viewLegendData";

export interface SliceSourceEditorProps {
  source: string;
  filePath?: string | null;
  highlightLines: number[];
  criterionLine?: number;
}

export function SliceSourceEditor({
  source,
  filePath,
  highlightLines,
  criterionLine,
}: SliceSourceEditorProps) {
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
