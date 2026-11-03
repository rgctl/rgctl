/**
 * Pure layout for the Taint path stage: ordered steps from source → sink.
 * Assertable without WebGL / canvas.
 */
import type { TaintFlowView } from "./data/loadTaint";

export type PathStepKind = "source" | "step" | "sink" | "sanitizer";

export interface TaintPathStep {
  index: number;
  kind: PathStepKind;
  line: number;
  statement: string;
  /** 0..1 along the path for positioning */
  t: number;
}

export interface TaintPathModel {
  steps: TaintPathStep[];
  variable: string;
  sourceType: string;
  sinkType: string;
  vulnerable: boolean;
  severity: number;
  sanitizers: string[];
}

export function buildTaintPathModel(flow: TaintFlowView): TaintPathModel {
  const n = Math.max(flow.path_statements.length, flow.path_lines.length, 1);
  const steps: TaintPathStep[] = [];

  for (let i = 0; i < n; i++) {
    const statement = flow.path_statements[i] ?? "";
    const line = flow.path_lines[i] ?? 0;
    let kind: PathStepKind = "step";
    if (i === 0) kind = "source";
    else if (i === n - 1) kind = "sink";
    steps.push({
      index: i,
      kind,
      line,
      statement,
      t: n === 1 ? 0.5 : i / (n - 1),
    });
  }

  // Annotate sanitizer presence as a badge on the model (not a fake path node).
  return {
    steps,
    variable: flow.variable,
    sourceType: flow.source_type,
    sinkType: flow.sink_type,
    vulnerable: flow.vulnerable,
    severity: flow.severity,
    sanitizers: flow.sanitizers ?? [],
  };
}

/** SVG-friendly positions along a horizontal sequence. */
export function pathPositions(
  model: TaintPathModel,
  width: number,
  height: number,
  padX = 48,
): { x: number; y: number; step: TaintPathStep }[] {
  const usable = Math.max(width - padX * 2, 1);
  const y = height / 2;
  return model.steps.map((step) => ({
    x: padX + step.t * usable,
    y,
    step,
  }));
}
