/**
 * Static rules room state — index, filters, selected file shard, session findings hint.
 */
import { useEffect, useMemo, useState } from "react";
import { loadManifest, type Manifest } from "../data";
import {
  ALL_KANTRA_CATEGORIES,
  fileMatchesFilters,
  loadKantraFileBundle,
  loadKantraIndex,
  loadKantraSourceText,
  loadSessionKantraFindings,
  snippetRange,
  violationKey,
  violationMatchesFilters,
  type KantraFileBundle,
  type KantraFilterCategory,
  type KantraIndexPayload,
  type KantraViolation,
} from "../data/loadKantra";

export type RulesRoomView =
  | { kind: "loading" }
  | { kind: "missing" }
  | { kind: "stale" }
  | { kind: "empty"; index: KantraIndexPayload }
  | {
      kind: "ready";
      index: KantraIndexPayload;
      filteredFiles: ReturnType<typeof filterFiles>;
      selectedFileId: string | null;
      fileBundle: KantraFileBundle | null;
      fileLoading: boolean;
      filteredViolations: { violation: KantraViolation; key: string; index: number }[];
      selectedViolationKey: string | null;
      selectedViolation: KantraViolation | null;
      sourceSnippet: { text: string; firstLine: number } | null;
      sourceLoading: boolean;
    }
  | { kind: "error"; message: string };

function filterFiles(
  index: KantraIndexPayload,
  categories: ReadonlySet<KantraFilterCategory>,
  target: string | null,
  search: string,
) {
  const q = search.trim().toLowerCase();
  return index.files.filter((f) => {
    if (!fileMatchesFilters(f, categories, target)) return false;
    if (!q) return true;
    return f.path.toLowerCase().includes(q);
  });
}

export interface UseStaticRulesRoom {
  view: RulesRoomView;
  categoryFilters: Set<KantraFilterCategory>;
  toggleCategory: (c: KantraFilterCategory) => void;
  setAllCategories: (on: boolean) => void;
  targetFilter: string | null;
  setTargetFilter: (t: string | null) => void;
  sourceFilter: string | null;
  setSourceFilter: (s: string | null) => void;
  highConfidenceOnly: boolean;
  setHighConfidenceOnly: (on: boolean) => void;
  showSkipped: boolean;
  setShowSkipped: (on: boolean) => void;
  targetSearch: string;
  setTargetSearch: (s: string) => void;
  fileSearch: string;
  setFileSearch: (s: string) => void;
  filePage: number;
  setFilePage: (n: number) => void;
  selectFile: (id: string | null) => void;
  selectViolation: (key: string | null) => void;
}

export function useStaticRulesRoom(): UseStaticRulesRoom {
  const [manifest, setManifest] = useState<Manifest | null>(null);
  const [sessionFindings, setSessionFindings] = useState<boolean | null>(null);
  const [index, setIndex] = useState<KantraIndexPayload | null>(null);
  const [loadKind, setLoadKind] = useState<
    "loading" | "missing" | "stale" | "empty" | "ready" | "error"
  >("loading");
  const [errorMessage, setErrorMessage] = useState<string | null>(null);

  const [categoryFilters, setCategoryFilters] = useState<Set<KantraFilterCategory>>(
    () => new Set(ALL_KANTRA_CATEGORIES),
  );
  const [targetFilter, setTargetFilter] = useState<string | null>(null);
  const [sourceFilter, setSourceFilter] = useState<string | null>(null);
  const [highConfidenceOnly, setHighConfidenceOnly] = useState(false);
  const [showSkipped, setShowSkipped] = useState(false);
  const [targetSearch, setTargetSearch] = useState("");
  const [fileSearch, setFileSearch] = useState("");
  const [filePage, setFilePage] = useState(0);
  const [selectedFileId, setSelectedFileId] = useState<string | null>(null);
  const [fileBundle, setFileBundle] = useState<KantraFileBundle | null>(null);
  const [fileLoading, setFileLoading] = useState(false);
  const [selectedViolationKey, setSelectedViolationKey] = useState<string | null>(null);
  const [sourceText, setSourceText] = useState<string | null>(null);
  const [sourceLoading, setSourceLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setLoadKind("loading");
    void Promise.all([loadManifest(), loadSessionKantraFindings()]).then(
      async ([m, findings]) => {
        if (cancelled) return;
        setManifest(m);
        setSessionFindings(findings);
        const section = m?.kantra ?? null;
        try {
          const loaded = await loadKantraIndex(section);
          if (cancelled) return;
          if (loaded.kind === "ready") {
            setIndex(loaded.index);
            setLoadKind("ready");
            return;
          }
          if (loaded.kind === "empty") {
            setIndex(loaded.index);
            setLoadKind("empty");
            return;
          }
          // unavailable / missing index
          const findingsPresent = findings === true;
          setIndex(null);
          setLoadKind(findingsPresent ? "stale" : "missing");
        } catch (e) {
          if (cancelled) return;
          setErrorMessage(e instanceof Error ? e.message : String(e));
          setLoadKind("error");
        }
      },
    );
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!index || !selectedFileId) {
      setFileBundle(null);
      setSelectedViolationKey(null);
      setSourceText(null);
      return;
    }
    let cancelled = false;
    setFileLoading(true);
    setFileBundle(null);
    setSelectedViolationKey(null);
    setSourceText(null);
    const detailDir = index.detail_dir || "kantra";
    void loadKantraFileBundle(detailDir, selectedFileId).then((bundle) => {
      if (cancelled) return;
      setFileBundle(bundle);
      setFileLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [index, selectedFileId]);

  const rulesById = useMemo(() => {
    const map = new Map<string, KantraIndexPayload["rules"][number]>();
    if (!index) return map;
    for (const r of index.rules) map.set(r.rule_id, r);
    return map;
  }, [index]);

  const filteredFiles = useMemo(() => {
    if (!index) return [];
    return filterFiles(index, categoryFilters, targetFilter, fileSearch);
  }, [index, categoryFilters, targetFilter, fileSearch]);

  useEffect(() => {
    setFilePage(0);
  }, [categoryFilters, targetFilter, fileSearch]);

  const filteredViolations = useMemo(() => {
    if (!fileBundle) return [];
    return fileBundle.violations
      .map((violation, i) => ({
        violation,
        index: i,
        key: violationKey(violation, i),
      }))
      .filter(({ violation }) =>
        violationMatchesFilters(
          violation,
          rulesById.get(violation.rule_id),
          categoryFilters,
          targetFilter,
          sourceFilter,
          highConfidenceOnly,
        ),
      );
  }, [
    fileBundle,
    rulesById,
    categoryFilters,
    targetFilter,
    sourceFilter,
    highConfidenceOnly,
  ]);

  const selectedViolation =
    filteredViolations.find((v) => v.key === selectedViolationKey)?.violation ?? null;

  useEffect(() => {
    if (!selectedViolation || !fileBundle) {
      setSourceText(null);
      return;
    }
    let cancelled = false;
    setSourceLoading(true);
    void loadKantraSourceText(fileBundle.source_id, fileBundle.path).then((text) => {
      if (cancelled) return;
      setSourceText(text);
      setSourceLoading(false);
    });
    return () => {
      cancelled = true;
    };
  }, [selectedViolation, fileBundle]);

  const sourceSnippet =
    selectedViolation && sourceText
      ? snippetRange(sourceText, selectedViolation.line)
      : null;

  let view: RulesRoomView;
  if (loadKind === "loading") view = { kind: "loading" };
  else if (loadKind === "error") view = { kind: "error", message: errorMessage ?? "unknown error" };
  else if (loadKind === "missing") view = { kind: "missing" };
  else if (loadKind === "stale") view = { kind: "stale" };
  else if (loadKind === "empty" && index) view = { kind: "empty", index };
  else if (loadKind === "ready" && index) {
    view = {
      kind: "ready",
      index,
      filteredFiles,
      selectedFileId,
      fileBundle,
      fileLoading,
      filteredViolations,
      selectedViolationKey,
      selectedViolation,
      sourceSnippet,
      sourceLoading,
    };
  } else {
    view = { kind: "missing" };
  }

  // silence unused — sessionFindings / manifest kept for future instrument chips
  void sessionFindings;
  void manifest;

  return {
    view,
    categoryFilters,
    toggleCategory: (c) => {
      setCategoryFilters((prev) => {
        const next = new Set(prev);
        if (next.has(c)) next.delete(c);
        else next.add(c);
        return next;
      });
    },
    setAllCategories: (on) => {
      setCategoryFilters(on ? new Set(ALL_KANTRA_CATEGORIES) : new Set());
    },
    targetFilter,
    setTargetFilter,
    sourceFilter,
    setSourceFilter,
    highConfidenceOnly,
    setHighConfidenceOnly,
    showSkipped,
    setShowSkipped,
    targetSearch,
    setTargetSearch,
    fileSearch,
    setFileSearch,
    filePage,
    setFilePage,
    selectFile: setSelectedFileId,
    selectViolation: setSelectedViolationKey,
  };
}
