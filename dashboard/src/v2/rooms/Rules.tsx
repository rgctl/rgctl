/**
 * Static rules room — Konveyor Kantra triage (v1 Migration Rules ported into v2 chrome).
 *
 * File-centric: filters → file list → violations → snippet. No Focus required.
 */
import { Chip, Panel, Unavailable } from "../shell";
import { highlightLine, TOKEN_CLASS } from "../highlight";
import { languageFor } from "./SourcePane";
import {
  KANTRA_FILTER_CATEGORIES,
  normalizeCategory,
  shortPath,
  type KantraFilterCategory,
  type KantraIndexPayload,
  type KantraRuleEntry,
  type KantraViolation,
} from "../data/loadKantra";
import { useStaticRulesRoom } from "../hooks/useStaticRulesRoom";

const FILE_PAGE_SIZE = 30;

const DISCOVER_HINT =
  "`rgctl discover . --with-kantra --with-universe` (or `rgctl rules run` then re-export with `--with-universe`)";

function categoryTone(
  category: string | null | undefined,
): "rose" | "amber" | "blue" | "zinc" {
  switch (normalizeCategory(category)) {
    case "mandatory":
      return "rose";
    case "potential":
      return "amber";
    case "optional":
      return "blue";
    default:
      return "zinc";
  }
}

export function RulesRoom() {
  const room = useStaticRulesRoom();
  const { view } = room;

  if (view.kind === "loading") {
    return (
      <div data-testid="rules-room-loading">
        <Panel title="Static rules" subtitle="reading the bundle">
          <p className="text-[11px] text-zinc-500">Reading `kantra_index.json`…</p>
        </Panel>
      </div>
    );
  }

  if (view.kind === "error") {
    return (
      <Panel title="Static rules" subtitle="the index could not be read">
        <Unavailable what="Kantra index failed to load" why={view.message} />
      </Panel>
    );
  }

  if (view.kind === "missing") {
    return (
      <div data-testid="rules-room-missing">
        <Panel title="Static rules" subtitle="no Kantra export in this bundle">
          <Unavailable
            tag="not exported"
            what="This bundle carries no Kantra static-rule export"
            why={`Run ${DISCOVER_HINT}. A bundle without that export shows this absence rather than an empty table.`}
          />
        </Panel>
      </div>
    );
  }

  if (view.kind === "stale") {
    return (
      <div data-testid="rules-room-stale">
        <Panel title="Static rules" subtitle="findings on disk, bundle export missing">
          <Unavailable
            tag="stale export"
            what="Session Kantra findings exist, but this universe bundle has no Kantra index"
            why={`Re-export the dashboard so triage can use the index/shards: ${DISCOVER_HINT}. An empty file list would lie about zero hits — this state is not “no violations”.`}
          />
        </Panel>
      </div>
    );
  }

  if (view.kind === "empty") {
    return (
      <div data-testid="rules-room-empty">
        <Panel title="Static rules" subtitle="evaluation ran, nothing matched">
          <Unavailable
            tag="zero hits"
            what="Kantra evaluation produced no violations"
            why={`The catalog ran (${view.index.evaluated_rules.toLocaleString()} rules, catalog ${view.index.catalog_id ?? view.index.ruleset}). Nothing matched this tree under the labels that were evaluated.`}
          />
        </Panel>
      </div>
    );
  }

  const pageCount = Math.max(1, Math.ceil(view.filteredFiles.length / FILE_PAGE_SIZE));
  const safePage = Math.min(room.filePage, pageCount - 1);
  const pageFiles = view.filteredFiles.slice(
    safePage * FILE_PAGE_SIZE,
    (safePage + 1) * FILE_PAGE_SIZE,
  );

  const showTargetFilter =
    !view.index.target_filter && (view.index.available_targets?.length ?? 0) > 0;
  const allTargets = view.index.available_targets ?? [];
  const q = room.targetSearch.trim().toLowerCase();
  const targets = q
    ? allTargets.filter((t) => t.target.toLowerCase().includes(q))
    : allTargets;

  const selectedRule: KantraRuleEntry | null = view.selectedViolation
    ? (view.index.rules.find((r) => r.rule_id === view.selectedViolation!.rule_id) ?? null)
    : null;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2" data-testid="rules-room">
      <Panel
        title="Static rules"
        subtitle={`${view.index.violation_count.toLocaleString()} violations · ${view.index.evaluated_rules.toLocaleString()} rules · ${view.index.file_count.toLocaleString()} files`}
        right={
          <div className="flex flex-wrap items-center gap-1">
            {view.index.catalog_id && <Chip tone="zinc">{view.index.catalog_id}</Chip>}
            {view.index.target_filter && (
              <Chip tone="blue">target: {view.index.target_filter}</Chip>
            )}
            {(view.index.skipped_rules?.length ?? 0) > 0 && (
              <button
                type="button"
                data-testid="rules-skipped-chip"
                onClick={() => room.setShowSkipped(!room.showSkipped)}
                className="rounded-full"
              >
                <Chip tone="amber">
                  {view.index.skipped_rules!.length} skipped
                </Chip>
              </button>
            )}
          </div>
        }
      >
        <CategoryBar
          index={view.index}
          selected={room.categoryFilters}
          onToggle={room.toggleCategory}
          onAll={() => room.setAllCategories(true)}
          onNone={() => room.setAllCategories(false)}
        />
        {showTargetFilter && (
          <TargetBar
            targets={targets}
            totalTargets={(view.index.available_targets ?? []).length}
            selected={room.targetFilter}
            search={room.targetSearch}
            onSearch={room.setTargetSearch}
            onSelect={room.setTargetFilter}
            violationCount={view.index.violation_count}
          />
        )}
        {(view.index.available_sources?.length ?? 0) > 0 && (
          <div className="mb-2 flex flex-wrap items-center gap-2" data-testid="rules-source-filter">
            <span className="text-[10px] uppercase tracking-wide text-zinc-500">Source</span>
            <select
              aria-label="Filter by source technology"
              className="rounded border border-white/10 bg-black/20 px-2 py-0.5 text-[11px] text-zinc-200"
              value={room.sourceFilter ?? ""}
              onChange={(e) =>
                room.setSourceFilter(e.target.value ? e.target.value : null)
              }
            >
              <option value="">All sources</option>
              {view.index.available_sources!.map((s) => (
                <option key={s} value={s}>
                  {s}
                </option>
              ))}
            </select>
            <label className="flex cursor-pointer items-center gap-1 text-[11px] text-zinc-300">
              <input
                type="checkbox"
                checked={room.highConfidenceOnly}
                onChange={(e) => room.setHighConfidenceOnly(e.target.checked)}
                className="accent-sky-500"
                data-testid="rules-high-confidence"
              />
              High confidence only
            </label>
          </div>
        )}
        {!(view.index.available_sources?.length) && (
          <div className="mb-2">
            <label className="flex cursor-pointer items-center gap-1 text-[11px] text-zinc-300">
              <input
                type="checkbox"
                checked={room.highConfidenceOnly}
                onChange={(e) => room.setHighConfidenceOnly(e.target.checked)}
                className="accent-sky-500"
                data-testid="rules-high-confidence"
              />
              High confidence only
            </label>
          </div>
        )}
        {room.showSkipped && (view.index.skipped_rules?.length ?? 0) > 0 && (
          <div
            className="mb-2 max-h-40 overflow-auto rounded border border-white/10 bg-black/20 p-2"
            data-testid="rules-skipped-list"
          >
            <table className="w-full text-left text-[11px]">
              <thead className="text-[10px] uppercase tracking-wide text-zinc-500">
                <tr>
                  <th className="px-1 py-1">Rule</th>
                  <th className="px-1 py-1">Reason</th>
                </tr>
              </thead>
              <tbody>
                {view.index.skipped_rules!.map((s) => (
                  <tr key={s.rule_id} className="border-t border-white/5">
                    <td className="px-1 py-1 font-mono text-[10px] text-zinc-200">
                      {s.rule_id}
                    </td>
                    <td className="px-1 py-1 text-zinc-400">{s.reason}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Panel>

      <div className="grid min-h-0 flex-1 gap-2 lg:grid-cols-[minmax(220px,280px)_1fr]">
        <Panel title="Files" subtitle={`${view.filteredFiles.length.toLocaleString()} matching`}>
          <input
            type="search"
            value={room.fileSearch}
            onChange={(e) => room.setFileSearch(e.target.value)}
            placeholder="Search paths…"
            aria-label="Search Kantra files"
            className="mb-2 w-full rounded border border-white/10 bg-black/20 px-2 py-1 text-[11px] text-zinc-200"
          />
          <ul className="max-h-[40vh] space-y-0.5 overflow-y-auto lg:max-h-[calc(100vh-22rem)]" data-testid="rules-file-list">
            {pageFiles.map((f) => {
              const active = f.id === view.selectedFileId;
              return (
                <li key={f.id}>
                  <button
                    type="button"
                    onClick={() => room.selectFile(f.id)}
                    className={`flex w-full flex-col rounded px-2 py-1.5 text-left text-[11px] ${
                      active
                        ? "bg-sky-500/20 text-sky-100"
                        : "text-zinc-300 hover:bg-white/[0.05]"
                    }`}
                  >
                    <span className="truncate font-medium" title={f.path}>
                      {shortPath(f.path)}
                    </span>
                    <span className="text-[10px] text-zinc-500">
                      {f.count} hit{f.count === 1 ? "" : "s"}
                      {f.max_blast > 0 ? ` · blast ${f.max_blast.toFixed(1)}` : ""}
                    </span>
                  </button>
                </li>
              );
            })}
            {pageFiles.length === 0 && (
              <li className="px-2 py-2 text-[11px] text-zinc-500">No files match these filters.</li>
            )}
          </ul>
          {pageCount > 1 && (
            <div className="mt-2 flex items-center gap-2 text-[10px] text-zinc-500">
              <button
                type="button"
                className="rounded border border-white/10 px-2 py-0.5 disabled:opacity-40"
                disabled={safePage <= 0}
                onClick={() => room.setFilePage(safePage - 1)}
              >
                Prev
              </button>
              <span>
                {safePage + 1}/{pageCount}
              </span>
              <button
                type="button"
                className="rounded border border-white/10 px-2 py-0.5 disabled:opacity-40"
                disabled={safePage >= pageCount - 1}
                onClick={() => room.setFilePage(safePage + 1)}
              >
                Next
              </button>
            </div>
          )}
        </Panel>

        <div className="flex min-h-0 flex-col gap-2">
          {!view.selectedFileId && (
            <p className="text-[11px] text-zinc-500">Select a file to review its rule violations.</p>
          )}
          {view.selectedFileId && view.fileLoading && (
            <p className="text-[11px] text-zinc-500">Loading file violations…</p>
          )}
          {view.selectedFileId && view.fileBundle && !view.fileLoading && (
            <>
              <Panel
                title={shortPath(view.fileBundle.path)}
                subtitle={`${view.filteredViolations.length} violation${view.filteredViolations.length === 1 ? "" : "s"}`}
              >
                <div className="max-h-[36vh] overflow-auto" data-testid="rules-violations">
                  {view.filteredViolations.length === 0 ? (
                    <p className="px-1 py-2 text-[11px] text-zinc-500">
                      No violations match the selected categories for this file.
                    </p>
                  ) : (
                    <table className="w-full text-left text-[11px]">
                      <thead className="sticky top-0 bg-[#12151b] text-[10px] uppercase tracking-wide text-zinc-500">
                        <tr>
                          <th className="px-1 py-1">Rule</th>
                          <th className="px-1 py-1">Line</th>
                          <th className="px-1 py-1">Category</th>
                          <th className="px-1 py-1">Effort</th>
                          <th className="px-1 py-1">Confidence</th>
                          <th className="px-1 py-1">Blast</th>
                          <th className="px-1 py-1">Symbol</th>
                        </tr>
                      </thead>
                      <tbody>
                        {view.filteredViolations.map(({ violation, key }) => {
                          const active = key === view.selectedViolationKey;
                          const ruleMeta = view.index.rules.find(
                            (r) => r.rule_id === violation.rule_id,
                          );
                          const effort = violation.effort ?? ruleMeta?.effort ?? null;
                          const blast =
                            violation.enrichment?.blast_radius_score ?? null;
                          return (
                            <tr
                              key={key}
                              className={`cursor-pointer border-t border-white/5 ${
                                active ? "bg-sky-500/15" : "hover:bg-white/[0.04]"
                              }`}
                              onClick={() => room.selectViolation(key)}
                            >
                              <td className="px-1 py-1 font-mono text-[10px] text-zinc-200">
                                {violation.rule_id}
                              </td>
                              <td className="px-1 py-1 tabular-nums">{violation.line}</td>
                              <td className="px-1 py-1">
                                <Chip tone={categoryTone(violation.category)}>
                                  {normalizeCategory(violation.category)}
                                </Chip>
                              </td>
                              <td className="px-1 py-1 tabular-nums text-zinc-400">
                                {effort != null ? effort : "—"}
                              </td>
                              <td className="px-1 py-1">
                                {violation.confidence === "low" ? (
                                  <Chip tone="amber">low</Chip>
                                ) : violation.confidence === "high" ? (
                                  <Chip tone="blue">high</Chip>
                                ) : (
                                  "—"
                                )}
                              </td>
                              <td className="px-1 py-1 tabular-nums text-zinc-400">
                                {blast != null ? blast.toFixed(1) : "—"}
                              </td>
                              <td className="px-1 py-1 text-zinc-400">
                                {violation.symbol ? (
                                  <code className="text-[10px]">{violation.symbol}</code>
                                ) : (
                                  "—"
                                )}
                              </td>
                            </tr>
                          );
                        })}
                      </tbody>
                    </table>
                  )}
                </div>
              </Panel>

              {view.selectedViolation && (
                <ViolationDetail
                  violation={view.selectedViolation}
                  rule={selectedRule}
                  snippet={view.sourceSnippet}
                  sourceLoading={view.sourceLoading}
                  filePath={view.fileBundle.path}
                />
              )}
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function CategoryBar({
  index,
  selected,
  onToggle,
  onAll,
  onNone,
}: {
  index: KantraIndexPayload;
  selected: ReadonlySet<KantraFilterCategory>;
  onToggle: (c: KantraFilterCategory) => void;
  onAll: () => void;
  onNone: () => void;
}) {
  return (
    <div
      className="mb-2 flex flex-wrap items-center gap-2"
      data-testid="rules-category-filters"
    >
      <span className="text-[10px] uppercase tracking-wide text-zinc-500">Filter</span>
      {KANTRA_FILTER_CATEGORIES.map((cat) => (
        <label key={cat} className="flex cursor-pointer items-center gap-1 text-[11px] text-zinc-300">
          <input
            type="checkbox"
            checked={selected.has(cat)}
            onChange={() => onToggle(cat)}
            className="accent-sky-500"
          />
          <Chip tone={categoryTone(cat)}>
            {cat}: {(index.by_category[cat] ?? 0).toLocaleString()}
          </Chip>
        </label>
      ))}
      <button type="button" className="rounded border border-white/10 px-2 py-0.5 text-[10px]" onClick={onAll}>
        All
      </button>
      <button type="button" className="rounded border border-white/10 px-2 py-0.5 text-[10px]" onClick={onNone}>
        None
      </button>
      {selected.size === 0 && (
        <span className="text-[10px] text-amber-300">Select at least one category.</span>
      )}
    </div>
  );
}

function TargetBar({
  targets,
  totalTargets,
  selected,
  search,
  onSearch,
  onSelect,
  violationCount,
}: {
  targets: { target: string; violation_count: number }[];
  totalTargets: number;
  selected: string | null;
  search: string;
  onSearch: (s: string) => void;
  onSelect: (t: string | null) => void;
  violationCount: number;
}) {
  return (
    <div className="flex flex-wrap items-center gap-2" data-testid="rules-target-filters">
      <span className="text-[10px] uppercase tracking-wide text-zinc-500">Target</span>
      <select
        className="max-w-[18rem] rounded border border-white/10 bg-black/30 px-2 py-1 text-[11px] text-zinc-200"
        value={selected ?? ""}
        aria-label="Filter by Konveyor target"
        onChange={(e) => onSelect(e.target.value || null)}
      >
        <option value="">All targets ({violationCount.toLocaleString()})</option>
        {targets.map((t) => (
          <option key={t.target} value={t.target}>
            {t.target} ({t.violation_count.toLocaleString()})
          </option>
        ))}
      </select>
      {totalTargets > 12 && (
        <input
          type="search"
          value={search}
          onChange={(e) => onSearch(e.target.value)}
          placeholder="Search targets…"
          aria-label="Search Konveyor targets"
          className="max-w-[14rem] rounded border border-white/10 bg-black/20 px-2 py-1 text-[11px]"
        />
      )}
      {selected && (
        <button
          type="button"
          className="rounded border border-white/10 px-2 py-0.5 text-[10px]"
          onClick={() => onSelect(null)}
        >
          Clear target
        </button>
      )}
    </div>
  );
}

function ViolationDetail({
  violation,
  rule,
  snippet,
  sourceLoading,
  filePath,
}: {
  violation: KantraViolation;
  rule: KantraRuleEntry | null;
  snippet: { text: string; firstLine: number } | null;
  sourceLoading: boolean;
  filePath: string;
}) {
  const message = rule?.message ?? violation.message ?? null;
  const description = rule?.description ?? null;
  const targets = (violation.targets?.length ? violation.targets : rule?.targets) ?? [];
  const sources = (violation.sources?.length ? violation.sources : rule?.sources) ?? [];
  const links = rule?.links ?? [];
  const effort = violation.effort ?? rule?.effort ?? null;
  const enrichment = violation.enrichment;
  const lang = languageFor(filePath);

  return (
    <div data-testid="rules-violation-detail">
    <Panel
      title={violation.rule_id}
      subtitle={`line ${violation.line}${violation.matched_location ? ` · ${violation.matched_location}` : ""}`}
      right={
        <div className="flex flex-wrap gap-1">
          <Chip tone={categoryTone(violation.category)}>
            {normalizeCategory(violation.category)}
          </Chip>
          {violation.confidence === "low" && <Chip tone="amber">low confidence</Chip>}
          {violation.confidence === "high" && <Chip tone="blue">high confidence</Chip>}
        </div>
      }
    >
      {description && (
        <p className="mb-2 whitespace-pre-wrap text-[11px] text-zinc-400" data-testid="rules-description">
          {description}
        </p>
      )}
      {message && <p className="mb-2 whitespace-pre-wrap text-[11px] text-zinc-300">{message}</p>}
      {effort != null && (
        <p className="mb-2 text-[10px] text-zinc-500" data-testid="rules-effort">
          effort: {effort}
        </p>
      )}
      {targets.length > 0 && (
        <p className="mb-2 text-[10px] text-zinc-500">targets: {targets.join(", ")}</p>
      )}
      {sources.length > 0 && (
        <div className="mb-2 flex flex-wrap gap-1" data-testid="rules-sources">
          {sources.map((s) => (
            <Chip key={s} tone="zinc">
              {s}
            </Chip>
          ))}
        </div>
      )}
      {links.length > 0 && (
        <ul className="mb-2 space-y-1" data-testid="rules-links">
          {links.map((link) => (
            <li key={link.url}>
              <a
                href={link.url}
                target="_blank"
                rel="noopener noreferrer"
                className="text-[11px] text-sky-400 underline"
              >
                {link.title || link.url}
              </a>
            </li>
          ))}
        </ul>
      )}
      {enrichment && (
        <dl
          className="mb-2 grid grid-cols-2 gap-x-3 gap-y-1 text-[10px] text-zinc-500"
          data-testid="rules-enrichment"
        >
          {enrichment.blast_radius_score != null && (
            <>
              <dt>blast radius</dt>
              <dd className="tabular-nums text-zinc-300">
                {enrichment.blast_radius_score.toFixed(2)}
              </dd>
            </>
          )}
          {enrichment.impact_zone_size != null && (
            <>
              <dt>impact zone</dt>
              <dd className="tabular-nums text-zinc-300">{enrichment.impact_zone_size}</dd>
            </>
          )}
          {enrichment.community_id != null && (
            <>
              <dt>community</dt>
              <dd className="tabular-nums text-zinc-300">{enrichment.community_id}</dd>
            </>
          )}
          {enrichment.pagerank != null && (
            <>
              <dt>pagerank</dt>
              <dd className="tabular-nums text-zinc-300">{enrichment.pagerank.toFixed(4)}</dd>
            </>
          )}
        </dl>
      )}
      {sourceLoading && <p className="text-[11px] text-zinc-500">Loading source…</p>}
      {!sourceLoading && !snippet && (
        <p className="text-[11px] text-zinc-500">Source not in this bundle for this file.</p>
      )}
      {snippet && (
        <pre className="overflow-x-auto rounded border border-white/8 bg-black/30 p-2 text-[11px] leading-5">
          {snippet.text.split("\n").map((line, i) => {
            const lineNo = snippet.firstLine + i;
            const active = lineNo === violation.line;
            const tokens = highlightLine(line, lang);
            return (
              <div
                key={lineNo}
                className={`flex gap-3 ${active ? "bg-rose-500/15" : ""}`}
              >
                <span className="w-8 shrink-0 select-none text-right text-zinc-600 tabular-nums">
                  {lineNo}
                </span>
                <code className="whitespace-pre text-zinc-200">
                  {tokens.map((t, j) => (
                    <span key={j} className={TOKEN_CLASS[t.kind]}>
                      {t.text}
                    </span>
                  ))}
                </code>
              </div>
            );
          })}
        </pre>
      )}
    </Panel>
    </div>
  );
}
