/**
 * One colour per node kind, and the same colour every time.
 *
 * The level draws every node in the package — functions, classes, modules, files, variables, imports — and until
 * now they were all the same blue, so the picture could not answer the first question anyone asks of it: *what am
 * I looking at?* Colour is the cheapest channel for that, and it is only useful if it is **predictable**: the same
 * kind must be the same colour on every render, in every district, and in the list beside the drawing.
 *
 * So this is a fixed table, not a hash and not an order-of-appearance assignment. The kinds that do not have an
 * entry fall back to `OTHER_COLOUR`, and the room states the mapping in its own legend rather than expecting the
 * reader to infer it.
 *
 * Hue discipline: blue is the default for "a function" because that is what a district is mostly made of; the
 * type-like kinds share a violet family (they are all declarations of a thing, and giving each its own hue made a
 * level look like confetti); structures of the repository — files, modules, imports — are cool and desaturated, so
 * they recede; run-time data (variables, config keys) is warm, so it stands out against the cool majority.
 */

/** Kinds whose colour is worth naming, keyed by the engine's own `node_type_name`. */
const KIND_COLOURS: Record<string, string> = {
  function: "#8fd0ff", // the default case: a district is mostly functions
  class: "#c084fc", // type-like kinds share a family —
  struct: "#c084fc",
  interface: "#c084fc",
  enum: "#c084fc",
  typealias: "#c084fc",
  annotation: "#c084fc",
  module: "#5eead4", // repository structure: cool, receding
  file: "#94a3b8",
  import: "#64748b",
  dependency: "#64748b",
  variable: "#fbbf24", // run-time data: warm, stands out
  configkey: "#fb923c",
  table: "#fbbf24",
  macro: "#a3e635",
  job: "#a3e635",
  buildstep: "#a3e635",
};

/** Anything without an entry of its own. Deliberately dull: an unnamed kind should not compete for attention. */
export const OTHER_COLOUR = "#7c8794";

/** The colour for a kind, keyed case-insensitively because the string crosses a JSON boundary. */
export function colourForKind(kind: string | null | undefined): string {
  return KIND_COLOURS[(kind ?? "").trim().toLowerCase()] ?? OTHER_COLOUR;
}

/**
 * The legend, as the room states it: one entry per family, in the order they appear in a level's list.
 *
 * Named by family rather than by kind because fifteen entries do not fit beside a drawing, and "violet = a type
 * declaration" is the fact a reader needs; the list itself carries the exact kind of each row.
 */
export const KIND_LEGEND: ReadonlyArray<{ label: string; colour: string }> = [
  { label: "func", colour: KIND_COLOURS.function },
  { label: "type", colour: KIND_COLOURS.class },
  { label: "module", colour: KIND_COLOURS.module },
  { label: "file", colour: KIND_COLOURS.file },
  { label: "var", colour: KIND_COLOURS.variable },
  { label: "import", colour: KIND_COLOURS.import },
];

/** `0xrrggbb`, for three.js, from the same table the UI uses. */
export function colourHexForKind(kind: string | null | undefined): number {
  return Number.parseInt(colourForKind(kind).replace("#", ""), 16);
}
