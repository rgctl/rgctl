import type { CategoryFilter } from "./graphExplore";
import type { CommunitiesPayload, Metanode, SubgraphNode } from "./types";

export interface GraphSidebarProps {
  level: "metagraph" | "subgraph";
  communities: CommunitiesPayload | null;
  selectedCommunityId: number | null;
  onSelectCommunity: (id: number | null) => void;
  category: CategoryFilter;
  onCategoryChange: (category: CategoryFilter) => void;
  soloCommunity: boolean;
  onSoloCommunityChange: (solo: boolean) => void;
  visibleCount: number;
  totalCount: number;
  drillLabel: string | null;
  onBack: () => void;
  hover: Metanode | null;
  selected: Metanode | null;
  subHover: SubgraphNode | null;
  onDrill?: () => void;
  drilling?: boolean;
}

const CATEGORIES: Array<{ id: CategoryFilter; label: string }> = [
  { id: "all", label: "All packages" },
  { id: "functions", label: "Functions only" },
  { id: "classes", label: "Classes only" },
  { id: "both", label: "Mixed" },
];

export function GraphSidebar({
  level,
  communities,
  selectedCommunityId,
  onSelectCommunity,
  category,
  onCategoryChange,
  soloCommunity,
  onSoloCommunityChange,
  visibleCount,
  totalCount,
  drillLabel,
  onBack,
  hover,
  selected,
  subHover,
  onDrill,
  drilling,
}: GraphSidebarProps) {
  const focus = selected ?? hover;

  return (
    <aside class="graph-sidebar border-start bg-white">
      <div class="graph-sidebar-inner">
        <nav class="graph-breadcrumb px-3 py-2 border-bottom small" aria-label="Graph location">
          <button
            type="button"
            class={`btn btn-link btn-sm p-0 text-decoration-none ${level === "metagraph" ? "fw-semibold text-body" : ""}`}
            onClick={level === "subgraph" ? onBack : undefined}
            disabled={level === "metagraph"}
          >
            Packages
          </button>
          {level === "subgraph" && drillLabel && (
            <>
              <span class="text-muted mx-1">/</span>
              <span class="fw-semibold text-truncate d-inline-block graph-breadcrumb-leaf">
                {drillLabel}
              </span>
            </>
          )}
        </nav>

        {level === "metagraph" && (
          <>
            <section class="graph-sidebar-section px-3 py-2 border-bottom">
              <h3 class="graph-sidebar-heading">Communities</h3>
              <p class="text-muted small mb-2">
                {visibleCount} / {totalCount} packages visible
              </p>
              <div class="graph-sidebar-list">
                <button
                  type="button"
                  class={`graph-sidebar-item w-100 text-start ${selectedCommunityId === null ? "active" : ""}`}
                  onClick={() => onSelectCommunity(null)}
                >
                  <span class="graph-sidebar-swatch" style={{ background: "#6c757d" }} />
                  All communities
                </button>
                {(communities?.communities ?? []).map((c) => (
                  <button
                    key={c.id}
                    type="button"
                    class={`graph-sidebar-item w-100 text-start ${selectedCommunityId === c.id ? "active" : ""}`}
                    onClick={() => onSelectCommunity(c.id)}
                  >
                    <span class="graph-sidebar-swatch" style={{ background: c.color }} />
                    <span class="flex-grow-1 text-truncate">{c.label}</span>
                    <span class="text-muted small ms-1">{c.package_count}</span>
                  </button>
                ))}
              </div>
              {selectedCommunityId !== null && (
                <div class="form-check form-switch mt-2 mb-0">
                  <input
                    class="form-check-input"
                    type="checkbox"
                    id="solo-community"
                    checked={soloCommunity}
                    onChange={(e) => onSoloCommunityChange((e.target as HTMLInputElement).checked)}
                  />
                  <label class="form-check-label small" for="solo-community">
                    Solo community (hide cross-links)
                  </label>
                </div>
              )}
            </section>

            <section class="graph-sidebar-section px-3 py-2 border-bottom">
              <h3 class="graph-sidebar-heading">Categories</h3>
              <div class="d-flex flex-column gap-1">
                {CATEGORIES.map((opt) => (
                  <label key={opt.id} class="form-check small mb-0">
                    <input
                      class="form-check-input"
                      type="radio"
                      name="graph-category"
                      checked={category === opt.id}
                      onChange={() => onCategoryChange(opt.id)}
                    />
                    <span class="form-check-label">{opt.label}</span>
                  </label>
                ))}
              </div>
            </section>
          </>
        )}

        <section class="graph-sidebar-section px-3 py-3 flex-grow-1 min-h-0 overflow-auto">
          <h3 class="graph-sidebar-heading">Inspector</h3>
          {level === "subgraph" && subHover ? (
            <SubgraphDetail node={subHover} />
          ) : focus ? (
            <MetanodeDetail
              node={focus}
              isSelected={!!selected}
              onDrill={onDrill}
              drilling={drilling}
            />
          ) : (
            <p class="text-muted small mb-0">
              {level === "subgraph"
                ? "Hover a node for details."
                : "Hover or click a package. Double-click or Drill down to expand members."}
            </p>
          )}
        </section>
      </div>
    </aside>
  );
}

function MetanodeDetail({
  node,
  isSelected,
  onDrill,
  drilling,
}: {
  node: Metanode;
  isSelected: boolean;
  onDrill?: () => void;
  drilling?: boolean;
}) {
  return (
    <>
      <dl class="row small mb-0">
        <dt class="col-5 text-muted">{isSelected ? "Selected" : "Hover"}</dt>
        <dd class="col-7 mb-1">
          <code class="small text-break">{node.label}</code>
        </dd>
        {node.community_id != null && (
          <>
            <dt class="col-5 text-muted">Community</dt>
            <dd class="col-7 mb-1">{node.community_id}</dd>
          </>
        )}
        <dt class="col-5 text-muted">Members</dt>
        <dd class="col-7 mb-1">{node.size.toLocaleString()}</dd>
        <dt class="col-5 text-muted">Functions</dt>
        <dd class="col-7 mb-1">{node.functions.toLocaleString()}</dd>
        <dt class="col-5 text-muted">Classes</dt>
        <dd class="col-7 mb-1">{node.classes.toLocaleString()}</dd>
        <dt class="col-5 text-muted">Avg complexity</dt>
        <dd class="col-7 mb-1">{node.avg_complexity.toFixed(1)}</dd>
      </dl>
      {onDrill && (
        <button
          type="button"
          class="btn btn-primary btn-sm w-100 mt-3"
          disabled={drilling}
          onClick={onDrill}
        >
          {drilling ? "Expanding…" : "Drill down"}
        </button>
      )}
    </>
  );
}

function SubgraphDetail({ node }: { node: SubgraphNode }) {
  return (
    <dl class="row small mb-0">
      <dt class="col-5 text-muted">Name</dt>
      <dd class="col-7 mb-1">
        <code class="small text-break">{node.name}</code>
      </dd>
      <dt class="col-5 text-muted">Type</dt>
      <dd class="col-7 mb-1">{node.node_type_name}</dd>
      <dt class="col-5 text-muted">Complexity</dt>
      <dd class="col-7 mb-1">{node.complexity.toFixed(1)}</dd>
      {node.community_id != null && (
        <>
          <dt class="col-5 text-muted">Community</dt>
          <dd class="col-7 mb-1">{node.community_id}</dd>
        </>
      )}
      {node.file_path && (
        <>
          <dt class="col-5 text-muted">File</dt>
          <dd class="col-7 mb-1 text-break">{node.file_path}</dd>
        </>
      )}
    </dl>
  );
}
