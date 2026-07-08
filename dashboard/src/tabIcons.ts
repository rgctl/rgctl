import type { TabId } from "./tabDocs";

export interface TabDef {
  id: TabId;
  label: string;
  icon: string;
}

/** Tab nav icons — Bootstrap Icons equivalents of the legacy Font Awesome set. */
export const DASHBOARD_TABS: TabDef[] = [
  { id: "graph", label: "Graph Visualization", icon: "bi-diagram-3" },
  { id: "functions", label: "Functions", icon: "bi-code-slash" },
  { id: "cfg", label: "CFG / PDG Analysis", icon: "bi-bar-chart-line" },
  { id: "dataflow", label: "Dataflow", icon: "bi-bezier" },
  { id: "taint", label: "Taint Analysis", icon: "bi-bug" },
  { id: "guide", label: "Query Guide", icon: "bi-book" },
  { id: "slice", label: "Program Slicing", icon: "bi-scissors" },
  { id: "blast", label: "Blast Radius", icon: "bi-radioactive" },
];
