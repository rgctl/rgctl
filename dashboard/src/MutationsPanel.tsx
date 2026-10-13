import { useMemo, useState } from "preact/hooks";
import type { MutationWriteEntry, MutationsIndexPayload } from "./types";

export interface MutationSelectTarget {
  functionId: string;
  line: number;
  variable: string | null;
  member: string;
}

interface MutationsPanelProps {
  index: MutationsIndexPayload | null;
  loadError: string | null;
  onSelect: (target: MutationSelectTarget) => void;
  activeKey: string | null;
}

function matchesType(have: string | null | undefined, want: string): boolean {
  if (!want) return true;
  if (!have) return false;
  const h = have.trim();
  const w = want.trim();
  if (h === w) return true;
  return h.endsWith(`.${w}`) || h.endsWith(`::${w}`);
}

export function filterMutationWrites(
  writes: MutationWriteEntry[],
  typeName: string,
  excludeCtors: boolean,
  member: string,
  includeUnresolved: boolean,
): MutationWriteEntry[] {
  const wantMember = member.trim();
  const wantType = typeName.trim();
  return writes.filter((w) => {
    if (excludeCtors && w.is_constructor) return false;
    if (wantMember && w.member !== wantMember) return false;
    if (w.kind === "Unresolved") {
      if (!includeUnresolved) return false;
      return wantType.length === 0;
    }
    return matchesType(w.receiver_type, wantType);
  });
}

export function MutationsPanel({ index, loadError, onSelect, activeKey }: MutationsPanelProps) {
  // Empty default = show every indexed field write. Seeding this with a demo
  // type from another project made a working feature render "0 shown" on load.
  const [typeName, setTypeName] = useState("");
  const [member, setMember] = useState("");
  const [excludeCtors, setExcludeCtors] = useState(true);
  const [includeUnresolved, setIncludeUnresolved] = useState(false);

  const hits = useMemo(() => {
    if (!index?.available) return [];
    return filterMutationWrites(
      index.writes,
      typeName,
      excludeCtors,
      member,
      includeUnresolved,
    );
  }, [index, typeName, excludeCtors, member, includeUnresolved]);

  if (loadError) {
    return (
      <div class="mutations-panel border rounded bg-white p-2 small" data-testid="mutations-panel">
        <div class="text-warning mb-0">Mutations index: {loadError}</div>
      </div>
    );
  }

  if (!index) {
    return (
      <div class="mutations-panel border rounded bg-white p-2 small text-muted" data-testid="mutations-panel">
        Loading mutations index…
      </div>
    );
  }

  if (!index.available) {
    return (
      <div class="mutations-panel border rounded bg-white p-2 small" data-testid="mutations-panel">
        <div class="fw-semibold mb-1">Field mutations (CPG)</div>
        <p class="text-muted mb-0">
          No field-write index. Re-run{" "}
          <code>rgctl discover . --with-cfg --with-dashboard</code>.
        </p>
      </div>
    );
  }

  return (
    <div
      class="mutations-panel border rounded bg-white d-flex flex-column flex-shrink-0"
      data-testid="mutations-panel"
      style={{ maxHeight: "220px" }}
    >
      <div class="border-bottom py-2 px-3 small flex-shrink-0 d-flex flex-wrap align-items-end gap-2">
        <div class="fw-semibold me-auto">Field mutations (CPG)</div>
        <span class="text-muted">
          {index.write_count} indexed
          {index.truncated ? " (truncated)" : ""} · {hits.length} shown
        </span>
      </div>
      <div class="px-3 py-2 d-flex flex-wrap align-items-end gap-2 flex-shrink-0 border-bottom">
        <div style={{ minWidth: "160px" }}>
          <label class="form-label small mb-1" for="mut-type">
            Type
          </label>
          <input
            id="mut-type"
            class="form-control form-control-sm"
            list="mut-type-list"
            value={typeName}
            placeholder={index.types.length > 0 ? `e.g. ${index.types[0]}` : "(any)"}
            data-testid="mutations-type-input"
            onInput={(e) => setTypeName((e.target as HTMLInputElement).value)}
          />
          <datalist id="mut-type-list">
            {index.types.map((t) => (
              <option key={t} value={t} />
            ))}
          </datalist>
        </div>
        <div style={{ minWidth: "120px" }}>
          <label class="form-label small mb-1" for="mut-member">
            Member
          </label>
          <input
            id="mut-member"
            class="form-control form-control-sm"
            value={member}
            placeholder="(any)"
            data-testid="mutations-member-input"
            onInput={(e) => setMember((e.target as HTMLInputElement).value)}
          />
        </div>
        <div class="form-check mb-0">
          <input
            class="form-check-input"
            type="checkbox"
            id="mut-exclude-ctors"
            checked={excludeCtors}
            data-testid="mutations-exclude-ctors"
            onChange={(e) => setExcludeCtors((e.target as HTMLInputElement).checked)}
          />
          <label class="form-check-label small" for="mut-exclude-ctors">
            Exclude ctors
          </label>
        </div>
        <div class="form-check mb-0">
          <input
            class="form-check-input"
            type="checkbox"
            id="mut-unresolved"
            checked={includeUnresolved}
            onChange={(e) => setIncludeUnresolved((e.target as HTMLInputElement).checked)}
          />
          <label class="form-check-label small" for="mut-unresolved">
            Unresolved
          </label>
        </div>
      </div>
      <div class="flex-grow-1 min-h-0 overflow-auto small">
        {hits.length === 0 ? (
          <p class="text-muted px-3 py-2 mb-0" data-testid="mutations-empty">
            No hits for this filter (try another type, or clear the type for all typed writes).
          </p>
        ) : (
          <table class="table table-sm table-hover mb-0" data-testid="mutations-table">
            <thead class="table-light sticky-top">
              <tr>
                <th>Type</th>
                <th>Member</th>
                <th>Function</th>
                <th>Line</th>
                <th>Snippet</th>
              </tr>
            </thead>
            <tbody>
              {hits.map((w) => {
                const key = `${w.function_id}:${w.line}:${w.member}`;
                const active = key === activeKey;
                return (
                  <tr
                    key={key}
                    class={active ? "table-primary" : undefined}
                    style={{ cursor: "pointer" }}
                    data-testid="mutations-row"
                    data-function-id={w.function_id}
                    data-line={w.line}
                    onClick={() =>
                      onSelect({
                        functionId: w.function_id,
                        line: w.line,
                        variable: w.receiver_local ?? null,
                        member: w.member,
                      })
                    }
                  >
                    <td class="text-nowrap">{w.receiver_type ?? "—"}</td>
                    <td class="text-nowrap">
                      <code>{w.member}</code>
                    </td>
                    <td class="text-nowrap">{w.function_name}</td>
                    <td class="text-end text-muted">{w.line}</td>
                    <td class="text-truncate" style={{ maxWidth: "280px" }} title={w.code_snippet}>
                      {w.code_snippet}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
