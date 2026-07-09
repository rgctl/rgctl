/** Accessible column help — visible icon with hover/focus tooltip. */
export function ColumnHelp({
  text,
  placement = "below",
}: {
  text: string;
  placement?: "above" | "below";
}) {
  return (
    <span
      class={`functions-col-help functions-col-help--${placement}`}
      tabIndex={0}
      aria-label={text}
    >
      <i class="bi bi-info-circle functions-col-help-icon" aria-hidden="true" />
      <span class="functions-col-help-popup" role="tooltip">
        {text}
      </span>
    </span>
  );
}
