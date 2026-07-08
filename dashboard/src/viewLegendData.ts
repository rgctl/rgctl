import type { LegendEntry } from "./ViewLegend";

export const CFG_EDGE_COLORS: Record<string, string> = {
  next: "#6c757d",
  if_true: "#198754",
  if_false: "#dc3545",
  jump: "#fd7e14",
  return: "#0d6efd",
  exception: "#6f42c1",
};

export const CFG_NODE_COLORS = {
  entry: "#198754",
  exit: "#dc3545",
  block: "#0d6efd",
} as const;

export const CFG_EDGE_LEGEND: LegendEntry[] = Object.entries(CFG_EDGE_COLORS).map(([key, color]) => ({
  label: key.replaceAll("_", " "),
  color,
  kind: "line" as const,
}));

export const CFG_NODE_LEGEND: LegendEntry[] = [
  { label: "entry", color: CFG_NODE_COLORS.entry, kind: "dot" },
  { label: "exit", color: CFG_NODE_COLORS.exit, kind: "dot" },
  { label: "block", color: CFG_NODE_COLORS.block, kind: "dot" },
];

export const PDG_EDGE_COLORS = {
  data: "#198754",
  control: "#fd7e14",
} as const;

export const PDG_EDGE_LEGEND: LegendEntry[] = [
  { label: "data dependency", color: PDG_EDGE_COLORS.data, kind: "line" },
  { label: "control dependency", color: PDG_EDGE_COLORS.control, kind: "line" },
];

export const PDG_NODE_LEGEND: LegendEntry[] = [
  { label: "statement (L# = line)", color: "#0d6efd", kind: "dot" },
];

export const SLICE_HIGHLIGHT = {
  line: "rgba(13, 110, 253, 0.35)",
  criterion: "rgba(25, 135, 84, 0.45)",
} as const;

export const SLICE_LEGEND: LegendEntry[] = [
  { label: "slice statement", color: SLICE_HIGHLIGHT.line, kind: "square" },
  { label: "criterion line", color: SLICE_HIGHLIGHT.criterion, kind: "square" },
];

export const TAINT_SEVERITY_LEGEND: LegendEntry[] = [
  { label: "Critical (9+)", badgeClass: "bg-danger" },
  { label: "High (7–8)", badgeClass: "bg-warning text-dark" },
  { label: "Medium (5–6)", badgeClass: "bg-info text-dark" },
  { label: "Low (<5)", badgeClass: "bg-secondary" },
];

export const TAINT_STATUS_LEGEND: LegendEntry[] = [
  { label: "Vulnerable", badgeClass: "bg-danger" },
  { label: "Sanitized", badgeClass: "bg-success" },
];
