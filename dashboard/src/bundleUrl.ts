/**
 * Resolve dashboard data file URLs (graph_payload.bin, metagraph.json, …).
 *
 * Bundled workers live under `assets/`; discover writes data files one level up.
 * Relative fetch("./…") in a worker resolves against the worker script URL → 404.
 */
export function bundleDataUrl(filename: string): URL {
  if (typeof import.meta !== "undefined" && import.meta.url) {
    return new URL(filename, new URL("../", import.meta.url));
  }
  return new URL(filename, window.location.href);
}
