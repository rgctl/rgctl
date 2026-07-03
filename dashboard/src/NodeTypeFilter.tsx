import {
  DEFAULT_GRAPH_TYPE_MASK,
  NODE_TYPE_FILTER_OPTIONS,
} from "./types";

export interface NodeTypeFilterProps {
  mask: number;
  onChange: (mask: number) => void;
  disabled?: boolean;
}

export function NodeTypeFilter({ mask, onChange, disabled }: NodeTypeFilterProps) {
  const toggle = (bit: number) => {
    const next = (mask & bit) !== 0 ? mask & ~bit : mask | bit;
    onChange(next || bit);
  };

  return (
    <div class="type-filter" role="group" aria-label="Node type filter">
      {NODE_TYPE_FILTER_OPTIONS.map((opt) => (
        <label key={opt.label} class="type-filter-chip">
          <input
            type="checkbox"
            checked={(mask & opt.bit) !== 0}
            disabled={disabled}
            onChange={() => toggle(opt.bit)}
          />
          {opt.label}
        </label>
      ))}
    </div>
  );
}
