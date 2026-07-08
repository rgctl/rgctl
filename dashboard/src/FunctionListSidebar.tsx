import { useEffect, useRef, useState } from "preact/hooks";
import type { ComponentChildren } from "preact";
import {
  filterFunctionItems,
  FUNCTION_LIST_PAGE_SIZE,
  shortPath,
  type FunctionListItem,
} from "./functionListUtils";

export interface FunctionListSidebarProps {
  title?: string;
  count?: number;
  items: FunctionListItem[];
  selectedId: string | null;
  onSelect: (id: string) => void;
  loading?: boolean;
  emptyMessage?: string;
}

export function FunctionListLayout({
  sidebar,
  children,
}: {
  sidebar: ComponentChildren;
  children: ComponentChildren;
}) {
  return (
    <div class="function-list-layout d-flex flex-grow-1 min-h-0">
      {sidebar}
      <div class="function-list-main flex-grow-1 min-w-0 min-h-0 d-flex flex-column">{children}</div>
    </div>
  );
}

export function FunctionListSidebar({
  title = "Functions",
  count,
  items,
  selectedId,
  onSelect,
  loading,
  emptyMessage = "No functions match your search.",
}: FunctionListSidebarProps) {
  const [search, setSearch] = useState("");
  const [collapsed, setCollapsed] = useState(false);
  const [page, setPage] = useState(0);
  const [focusIndex, setFocusIndex] = useState(-1);
  const listRef = useRef<HTMLDivElement>(null);
  const searchRef = useRef<HTMLInputElement>(null);

  const filtered = filterFunctionItems(items, search);
  const pageCount = Math.max(1, Math.ceil(filtered.length / FUNCTION_LIST_PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const pageStart = safePage * FUNCTION_LIST_PAGE_SIZE;
  const pageItems = filtered.slice(pageStart, pageStart + FUNCTION_LIST_PAGE_SIZE);
  const totalLabel = count ?? items.length;
  const rangeStart = filtered.length === 0 ? 0 : pageStart + 1;
  const rangeEnd = Math.min(pageStart + FUNCTION_LIST_PAGE_SIZE, filtered.length);

  useEffect(() => {
    setPage(0);
    setFocusIndex(-1);
  }, [search]);

  useEffect(() => {
    setPage((current) => Math.min(current, Math.max(0, pageCount - 1)));
  }, [pageCount]);

  useEffect(() => {
    if (!selectedId) return;
    const idx = filtered.findIndex((item) => item.id === selectedId);
    if (idx < 0) return;
    const selectedPage = Math.floor(idx / FUNCTION_LIST_PAGE_SIZE);
    setPage(selectedPage);
    setFocusIndex(idx - selectedPage * FUNCTION_LIST_PAGE_SIZE);
  }, [selectedId, filtered]);

  const selectAt = (index: number) => {
    const item = pageItems[index];
    if (!item) return;
    setFocusIndex(index);
    onSelect(item.id);
  };

  const goPrev = () => {
    setPage((p) => Math.max(0, p - 1));
    setFocusIndex(-1);
  };

  const goNext = () => {
    setPage((p) => Math.min(pageCount - 1, p + 1));
    setFocusIndex(-1);
  };

  const onSearchKeyDown = (e: KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (pageItems.length > 0) selectAt(Math.max(0, focusIndex < 0 ? 0 : focusIndex));
      listRef.current?.focus();
    } else if (e.key === "Enter" && focusIndex >= 0) {
      e.preventDefault();
      selectAt(focusIndex);
    }
  };

  const onListKeyDown = (e: KeyboardEvent) => {
    if (pageItems.length === 0) return;
    if (e.key === "ArrowDown") {
      e.preventDefault();
      selectAt(Math.min(pageItems.length - 1, (focusIndex < 0 ? 0 : focusIndex) + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      if (focusIndex <= 0) {
        searchRef.current?.focus();
        setFocusIndex(-1);
      } else {
        selectAt(focusIndex - 1);
      }
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (focusIndex >= 0) selectAt(focusIndex);
    }
  };

  return (
    <aside
      class={`function-list-sidebar border-end bg-white ${collapsed ? "function-list-sidebar--collapsed" : ""}`}
      aria-label="Function list"
    >
      <div class="function-list-sidebar-inner">
        <div class="function-list-sidebar-header px-3 py-2 border-bottom d-flex align-items-center gap-2">
          <h2 class="function-list-sidebar-heading mb-0 flex-grow-1">{title}</h2>
          <span class="text-muted small">{totalLabel.toLocaleString()}</span>
          <button
            type="button"
            class="btn btn-sm btn-outline-secondary function-list-sidebar-toggle d-lg-none"
            aria-expanded={!collapsed}
            aria-label={collapsed ? "Show function list" : "Hide function list"}
            onClick={() => setCollapsed((v) => !v)}
          >
            {collapsed ? "»" : "«"}
          </button>
        </div>

        {!collapsed && (
          <>
            <div class="px-3 py-2 border-bottom flex-shrink-0">
              <input
                ref={searchRef}
                type="search"
                class="form-control form-control-sm"
                placeholder="Search functions…"
                value={search}
                onInput={(e) => {
                  setSearch((e.target as HTMLInputElement).value);
                  setFocusIndex(-1);
                }}
                onKeyDown={onSearchKeyDown}
                aria-label="Search functions"
              />
            </div>

            <div
              ref={listRef}
              class="function-list-scroll flex-grow-1 min-h-0 overflow-auto"
              tabIndex={0}
              role="listbox"
              aria-label="Functions"
              onKeyDown={onListKeyDown}
            >
              {loading && (
                <p class="text-muted small px-3 py-2 mb-0">Loading functions…</p>
              )}
              {!loading && filtered.length === 0 && (
                <p class="text-muted small px-3 py-2 mb-0">{emptyMessage}</p>
              )}
              {!loading &&
                pageItems.map((item, index) => {
                  const active = item.id === selectedId;
                  const focused = index === focusIndex;
                  return (
                    <button
                      key={item.id}
                      type="button"
                      role="option"
                      aria-selected={active}
                      class={`function-list-item w-100 text-start ${active ? "active" : ""} ${focused ? "focused" : ""}`}
                      onClick={() => selectAt(index)}
                      onMouseEnter={() => setFocusIndex(index)}
                    >
                      <div class="function-list-item-name text-truncate">{item.name}</div>
                      {(item.filePath || item.meta) && (
                        <div class="function-list-item-meta text-truncate">
                          {item.filePath && <span>{shortPath(item.filePath)}</span>}
                          {item.filePath && item.meta && <span class="mx-1">·</span>}
                          {item.meta && <span>{item.meta}</span>}
                        </div>
                      )}
                      {item.badge && (
                        <span class={`badge function-list-item-badge ${item.badgeClass ?? "bg-secondary"}`}>
                          {item.badge}
                        </span>
                      )}
                    </button>
                  );
                })}
            </div>

            {!loading && filtered.length > 0 && (
              <div class="function-list-pagination px-3 py-2 border-top flex-shrink-0 d-flex align-items-center justify-content-between gap-2">
                <button
                  type="button"
                  class="btn btn-sm btn-outline-secondary"
                  disabled={safePage <= 0}
                  aria-label="Previous page"
                  onClick={goPrev}
                >
                  Prev
                </button>
                <span class="small text-muted text-center flex-grow-1">
                  {rangeStart}–{rangeEnd} of {filtered.length.toLocaleString()}
                  {pageCount > 1 && ` · ${safePage + 1}/${pageCount}`}
                </span>
                <button
                  type="button"
                  class="btn btn-sm btn-outline-secondary"
                  disabled={safePage >= pageCount - 1}
                  aria-label="Next page"
                  onClick={goNext}
                >
                  Next
                </button>
              </div>
            )}
          </>
        )}
      </div>
    </aside>
  );
}
