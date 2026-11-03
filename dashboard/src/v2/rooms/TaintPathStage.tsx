/**
 * Center path stage: vertical sequence source → steps → sink (readable code, not truncated dots).
 */
import { useMemo } from "react";
import type { TaintFlowView } from "../data/loadTaint";
import { buildTaintPathModel } from "../taintPathLayout";
import { Chip } from "../shell";

export function TaintPathStage({ flow }: { flow: TaintFlowView | null }) {
  const model = useMemo(() => (flow ? buildTaintPathModel(flow) : null), [flow]);

  if (!flow || !model) {
    return (
      <div
        className="flex min-h-[200px] flex-1 items-center justify-center rounded-lg border border-dashed border-white/12 bg-[#060a12] p-6"
        data-testid="taint-path-empty"
      >
        <p className="max-w-sm text-center text-[12px] leading-relaxed text-zinc-500">
          Pick a row in the flows table. The path from source to sink appears here as a sequence of
          statements.
        </p>
      </div>
    );
  }

  return (
    <div
      className="flex min-h-[200px] flex-1 flex-col gap-3 overflow-y-auto rounded-lg border border-white/8 bg-[#060a12] p-4"
      data-testid="taint-path-stage"
    >
      <div className="flex flex-wrap items-center gap-1.5">
        <Chip tone={model.vulnerable ? "amber" : "green"}>
          {model.vulnerable ? "vulnerable" : "sanitized"}
        </Chip>
        <Chip tone="zinc">severity {model.severity}</Chip>
        <Chip tone="blue">
          <code>{model.variable}</code>
        </Chip>
        <span className="text-[10px] text-zinc-500">
          {model.sourceType} → {model.sinkType}
        </span>
        {model.sanitizers.length > 0 && (
          <Chip tone="green">sanitizers: {model.sanitizers.join(", ")}</Chip>
        )}
      </div>

      <ol className="mx-auto flex w-full max-w-xl flex-col gap-0" aria-label={`Taint path for ${model.variable}`}>
        {model.steps.map((step, i) => {
          const tone =
            step.kind === "source"
              ? "border-sky-500/35 bg-sky-500/10"
              : step.kind === "sink"
                ? "border-rose-500/35 bg-rose-500/10"
                : "border-violet-500/25 bg-violet-500/8";
          const label =
            step.kind === "source" ? "source" : step.kind === "sink" ? "sink" : `step ${step.index}`;
          return (
            <li key={step.index} className="flex flex-col items-stretch">
              <div className={`rounded-lg border px-3 py-2 ${tone}`}>
                <div className="mb-1 flex items-baseline justify-between gap-2">
                  <span className="text-[10px] uppercase tracking-wider text-zinc-400">{label}</span>
                  {step.line > 0 && (
                    <span className="font-mono text-[10px] text-zinc-500">L{step.line}</span>
                  )}
                </div>
                <pre className="whitespace-pre-wrap break-all font-mono text-[12px] leading-snug text-zinc-100">
                  {step.statement || "—"}
                </pre>
              </div>
              {i < model.steps.length - 1 && (
                <div className="flex items-center justify-center py-1" aria-hidden>
                  <div className="flex flex-col items-center">
                    <div className="h-3 w-px bg-white/20" />
                    <span className="text-[9px] text-zinc-600">▼</span>
                  </div>
                </div>
              )}
            </li>
          );
        })}
      </ol>
    </div>
  );
}
