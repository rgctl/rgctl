/**
 * Load taint index + Focus-aware function filter + selected flow bundle.
 */
import { useEffect, useMemo, useState } from "react";
import type { Focus } from "../focus";
import {
  blastForFunction,
  loadBlastIndex,
  loadDataflowIndex,
  loadTaintBundle,
  loadTaintIndex,
  type BlastIndexPayload,
  type DataflowIndexPayload,
  type TaintBundlePayload,
  type TaintFlowView,
  type TaintFunctionEntry,
  type TaintIndexLoad,
  type TaintIndexPayload,
} from "../data/loadTaint";

export interface TaintFilters {
  vulnerableOnly: boolean;
  minSeverity: number;
}

export type TaintRoomView =
  | { kind: "loading" }
  | { kind: "missing" }
  | { kind: "unavailable"; index: TaintIndexPayload }
  | { kind: "empty"; index: TaintIndexPayload }
  | {
      kind: "ready";
      index: TaintIndexPayload;
      /** Functions after Focus filter (before flow filters). */
      functions: TaintFunctionEntry[];
      focusMiss: boolean;
      activeFunctionId: string | null;
      bundle: TaintBundlePayload | null;
      bundleLoading: boolean;
      flows: TaintFlowView[];
      selectedFlowId: number | null;
      selectedFlow: TaintFlowView | null;
      dataflow: DataflowIndexPayload | null;
      blast: BlastIndexPayload | null;
      blastForActive: ReturnType<typeof blastForFunction>;
      dataflowForActive: DataflowIndexPayload["functions"][0] | null;
    };

export interface UseTaintRoom {
  view: TaintRoomView;
  filters: TaintFilters;
  setFilters: (f: TaintFilters) => void;
  setActiveFunctionId: (id: string | null) => void;
  setSelectedFlowId: (id: number | null) => void;
}

const DEFAULT_FILTERS: TaintFilters = { vulnerableOnly: false, minSeverity: 0 };

export function useTaintRoom(focus: Focus | null): UseTaintRoom {
  const [load, setLoad] = useState<TaintIndexLoad | null>(null);
  const [filters, setFilters] = useState<TaintFilters>(DEFAULT_FILTERS);
  const [activeFunctionId, setActiveFunctionId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<TaintBundlePayload | null>(null);
  const [bundleLoading, setBundleLoading] = useState(false);
  const [selectedFlowId, setSelectedFlowId] = useState<number | null>(null);
  const [dataflow, setDataflow] = useState<DataflowIndexPayload | null>(null);
  const [blast, setBlast] = useState<BlastIndexPayload | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoad(null);
    void Promise.all([loadTaintIndex(), loadDataflowIndex(), loadBlastIndex()]).then(
      ([taint, df, bl]) => {
        if (cancelled) return;
        setLoad(taint);
        setDataflow(df);
        setBlast(bl);
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  const functions = useMemo(() => {
    if (!load || load.kind !== "ready") return [] as TaintFunctionEntry[];
    if (!focus) return load.index.functions;
    return load.index.functions.filter((f) => f.function_id === focus);
  }, [load, focus]);

  const focusMiss = Boolean(
    focus && load?.kind === "ready" && !load.index.functions.some((f) => f.function_id === focus),
  );

  // Sync active function from Focus / list
  useEffect(() => {
    if (load?.kind !== "ready") return;
    if (focus && functions.length === 1) {
      setActiveFunctionId(functions[0]!.function_id);
      return;
    }
    if (focusMiss) {
      setActiveFunctionId(null);
      return;
    }
    if (!activeFunctionId && functions.length > 0) {
      setActiveFunctionId(functions[0]!.function_id);
    }
  }, [load, focus, functions, focusMiss, activeFunctionId]);

  useEffect(() => {
    if (!load || load.kind !== "ready" || !activeFunctionId) {
      setBundle(null);
      setSelectedFlowId(null);
      return;
    }
    let cancelled = false;
    setBundleLoading(true);
    void loadTaintBundle(load.index.detail_dir, activeFunctionId).then((b) => {
      if (cancelled) return;
      setBundle(b);
      setBundleLoading(false);
      const first = b?.flows[0];
      setSelectedFlowId(first ? first.id : null);
    });
    return () => {
      cancelled = true;
    };
  }, [load, activeFunctionId]);

  const flows = useMemo(() => {
    if (!bundle) return [] as TaintFlowView[];
    return bundle.flows.filter((f) => {
      if (filters.vulnerableOnly && !f.vulnerable) return false;
      if (f.severity < filters.minSeverity) return false;
      return true;
    });
  }, [bundle, filters]);

  const selectedFlow = useMemo(() => {
    if (selectedFlowId === null) return null;
    return flows.find((f) => f.id === selectedFlowId) ?? bundle?.flows.find((f) => f.id === selectedFlowId) ?? null;
  }, [flows, selectedFlowId, bundle]);

  // Keep selection inside filtered set when filters change
  useEffect(() => {
    if (selectedFlowId === null) return;
    if (flows.some((f) => f.id === selectedFlowId)) return;
    setSelectedFlowId(flows[0]?.id ?? null);
  }, [flows, selectedFlowId]);

  const view = useMemo((): TaintRoomView => {
    if (load === null) return { kind: "loading" };
    if (load.kind === "missing") return { kind: "missing" };
    if (load.kind === "unavailable") return { kind: "unavailable", index: load.index };
    if (load.kind === "empty") return { kind: "empty", index: load.index };
    const dataflowForActive =
      activeFunctionId && dataflow?.available
        ? (dataflow.functions.find((f) => f.function_id === activeFunctionId) ?? null)
        : null;
    return {
      kind: "ready",
      index: load.index,
      functions,
      focusMiss,
      activeFunctionId,
      bundle,
      bundleLoading,
      flows,
      selectedFlowId,
      selectedFlow,
      dataflow,
      blast,
      blastForActive: blastForFunction(blast, activeFunctionId ?? "", null),
      dataflowForActive,
    };
  }, [
    load,
    functions,
    focusMiss,
    activeFunctionId,
    bundle,
    bundleLoading,
    flows,
    selectedFlowId,
    selectedFlow,
    dataflow,
    blast,
  ]);

  return {
    view,
    filters,
    setFilters,
    setActiveFunctionId,
    setSelectedFlowId,
  };
}
