import { NODE_TYPE_FILTER_OPTIONS } from "./types";

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
    <div class="d-flex flex-wrap align-items-center gap-2">
      <span class="text-muted small me-1">Node types:</span>
      {NODE_TYPE_FILTER_OPTIONS.map((opt) => (
        <label key={opt.label} class="form-check form-check-inline mb-0 small">
          <input
            class="form-check-input"
            type="checkbox"
            checked={(mask & opt.bit) !== 0}
            disabled={disabled}
            onChange={() => toggle(opt.bit)}
          />
          <span class="form-check-label">{opt.label}</span>
        </label>
      ))}
    </div>
  );
}
