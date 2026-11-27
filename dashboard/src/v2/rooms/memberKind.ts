/**
 * What kind of graph node a level's member is, and whether it can have code.
 *
 * A district's members are **every node in the package** — the room asks the engine with `typeMask = 0`, which
 * `columnar.rs` reads as "keep every kind". So a level contains functions and also files, classes, modules,
 * variables, imports and config keys. Two things follow, and both were wrong before this module existed:
 *
 *   * a **file** node's name *is* its absolute path, so a list of five rows sorted by name showed five paths and
 *     looked broken;
 *   * opening one asked the CFG index for a function and reported "the CFG index does not reach it" — a statement
 *     about a missing *function* for a member that is not a function. The absence was real; the reason was not.
 *
 * Only `Function` has a body to open today. Everything else is a node the level can draw and label, and nothing
 * more, which is what the room now says.
 */

/**
 * The engine's own spelling of a function (`node_type_name` in `crates/rgctl-wasm/src/columnar.rs`).
 *
 * Compared case-insensitively because the string crosses a JSON boundary: today it is `"Function"`, and a rename
 * on either side must not turn every function into "not a function".
 */
const FUNCTION_KINDS = new Set(["function"]);

/** Whether this member is the kind of node that has a function body to open. */
export function isFunctionKind(kind: string | null | undefined): boolean {
  return FUNCTION_KINDS.has((kind ?? "").trim().toLowerCase());
}

/**
 * Order for the level's list: functions first, then everything else, each by name.
 *
 * Not a cosmetic preference. Sorting by name alone put absolute paths at the top — `/Users/…` sorts before any
 * identifier — so the five visible rows were the file nodes, i.e. exactly the members whose code cannot be opened.
 * The reader's first five rows should be the five they are most likely to want.
 */
export function compareMembers(
  a: { name: string; kind: string },
  b: { name: string; kind: string },
): number {
  const byFunction = Number(isFunctionKind(b.kind)) - Number(isFunctionKind(a.kind));
  if (byFunction !== 0) return byFunction;
  return a.name.localeCompare(b.name);
}

/**
 * Why a member's code cannot be opened, or `null` when it is a function and the index is worth asking.
 *
 * Two different absences, kept apart: "this is not a function" is a fact about the node, while "no indexed function
 * matches" is a fact about the index — and telling a reader the second when the first is true sends them looking
 * for a missing artifact that was never meant to exist.
 */
export function nonFunctionReason(kind: string | null | undefined, name: string): string | null {
  if (isFunctionKind(kind)) return null;
  const label = (kind ?? "").trim() || "node";
  return `${name} is a ${label} node, not a function — there is no function body to open. The level draws every node the package contains; the source and CFG views are for functions.`;
}

/**
 * A kind's name in the space of one list column: short, stable, and never a truncated fragment of a longer word.
 *
 * `FUNCTION` did not fit, so it rendered as `FUNCTI…` — the reader got three quarters of a word, no `func`, and a
 * column wide enough that it pushed the name it was labelling. Two rules: kinds with an obvious short word get it
 * (`func`, `var`), and anything else longer than six characters is cut at six with an ellipsis. Six, because
 * `module`, `struct`, `import` and `config` all fit and are the longest words worth keeping whole.
 */
const SHORT_KINDS: Record<string, string> = {
  function: "func",
  variable: "var",
  configkey: "config",
  typealias: "type",
  interface: "iface",
  annotation: "annot",
  dependency: "dep",
  buildstep: "build",
  puppetfact: "fact",
};

/** The column caption for a kind. Never wider than six characters. */
export function shortKind(kind: string | null | undefined): string {
  const key = (kind ?? "").trim().toLowerCase();
  if (key === "") return "node";
  const short = SHORT_KINDS[key] ?? key;
  return short.length > 6 ? `${short.slice(0, 6)}…` : short;
}
