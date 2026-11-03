/**
 * Telling "the drawing failed" apart from "this page is older than the server".
 *
 * The dashboard ships as content-hashed chunks: `index.v2-<hash>.js` lazily imports `UniverseScene-<hash>.js`.
 * `rgctl serve` serves the bundle embedded in the binary, so rebuilding the binary swaps the whole set — and a
 * page that was already open keeps its old entry script, whose lazy imports name chunks that no longer exist.
 * The result is a `Failed to fetch dynamically imported module: …/UniverseScene-rl4xgPwq.js` when the reader
 * navigates *back* into the room, which is both correct and completely unhelpful on its own.
 *
 * There is no way to make that import succeed — the bytes are gone — so the only honest thing the room can do is
 * name the cause and offer the one action that fixes it. Recognising the failure is the part worth testing: the
 * message comes from the browser, not from us, and each engine words it differently.
 */

/** The wordings browsers use for a lazy chunk that is no longer on the server. */
const PATTERNS = [
  /failed to fetch dynamically imported module/i, // Chromium, and Vite's own rethrow
  /error loading dynamically imported module/i, // Firefox
  /importing a module script failed/i, // Safari
  /unable to preload css/i, // Vite's stylesheet siblings of a lazy chunk
];

export function isStaleChunkError(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error ?? "");
  return PATTERNS.some((pattern) => pattern.test(message));
}

/** What the room says when the canvas it needs was built out from under the page. */
export const STALE_CHUNK_MESSAGE =
  "This page is running an earlier build of the dashboard than the server is serving, so the code that draws " +
  "the map is no longer there. Nothing is wrong with the bundle: reload to pick up the current build.";
