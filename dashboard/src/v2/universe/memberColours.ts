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
 * level look like confetti); the rest of the wheel is spent on the kinds that remain.
 *
 * **No greys, and nothing dark.** An earlier version made files and imports cool and desaturated "so they recede",
 * which on a near-black canvas meant they did not recede — they disappeared, and the two of them were the same
 * grey as each other anyway. Every colour here is saturated and mid-to-light, because the only alternative to
 * being seen is not being seen: a box that cannot be told apart from its neighbour is not a fact the picture is
 * carrying.
 */

/** Kinds whose colour is worth naming, keyed by the engine's own `node_type_name`. */
const KIND_COLOURS: Record<string, string> = {
  function: "#60a5fa", // blue: the default case, a district is mostly functions
  class: "#a78bfa", // one violet family for everything that declares a type —
  struct: "#a78bfa",
  interface: "#a78bfa",
  enum: "#a78bfa",
  typealias: "#a78bfa",
  annotation: "#a78bfa",
  module: "#2dd4bf", // teal: a namespace, one step above a file
  file: "#4ade80", // leaf green: the leaf of the tree, and nothing else is green
  import: "#facc15", // yellow: a dependency edge, the most common non-code node
  dependency: "#facc15",
  variable: "#fb923c", // orange: run-time data —
  table: "#fb923c",
  configkey: "#f87171", // red: configuration, which is neither code nor data
  macro: "#f472b6", // pink: generated or expanded code —
  job: "#f472b6",
  buildstep: "#f472b6",
};

/** Anything without an entry of its own: visible, and obviously not one of the named families. */
export const OTHER_COLOUR = "#e879f9";

/** The colour for a kind, keyed case-insensitively because the string crosses a JSON boundary. */
export function colourForKind(kind: string | null | undefined): string {
  return KIND_COLOURS[(kind ?? "").trim().toLowerCase()] ?? OTHER_COLOUR;
}

/**
 * The legend, as the room states it: one entry per family, in the order they appear in a level's list.
 *
 * Six families, not fifteen kinds: the list carries each row's exact kind, and the legend's job is only to say what
 * the colours on the boxes mean.
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
