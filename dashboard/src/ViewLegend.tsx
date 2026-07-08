import type { ComponentChildren } from "preact";

export type LegendSwatchKind = "dot" | "square" | "line";

export interface LegendEntry {
  label: string;
  color?: string;
  kind?: LegendSwatchKind;
  badgeClass?: string;
}

export interface ViewLegendProps {
  hint?: string;
  items?: LegendEntry[];
  children?: ComponentChildren;
  class?: string;
}

export function ViewLegend({ hint, items = [], children, class: className }: ViewLegendProps) {
  return (
    <div
      class={`view-legend px-2 py-1 bg-white small d-flex flex-wrap gap-2 gap-md-3 border-top align-items-center flex-shrink-0 ${className ?? ""}`}
    >
      {hint && <span class="text-muted">{hint}</span>}
      {items.map((item) => (
        <span key={item.label} class="view-legend-item d-inline-flex align-items-center gap-1">
          {item.badgeClass ? (
            <span class={`badge ${item.badgeClass}`} style={{ fontSize: "0.65rem", lineHeight: 1.1 }}>
              ●
            </span>
          ) : item.color ? (
            <LegendSwatch color={item.color} kind={item.kind ?? "square"} />
          ) : null}
          {item.label}
        </span>
      ))}
      {children}
    </div>
  );
}

function LegendSwatch({ color, kind }: { color: string; kind: LegendSwatchKind }) {
  if (kind === "line") {
    return <span class="view-legend-line" style={{ background: color }} />;
  }
  if (kind === "dot") {
    return <span class="view-legend-dot" style={{ background: color }} />;
  }
  return <span class="view-legend-square" style={{ background: color }} />;
}
