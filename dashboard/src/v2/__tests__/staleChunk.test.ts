/**
 * Recognising a stale build from the import failure it produces.
 *
 * This is the difference between the room saying "reload" and the room saying nothing useful. The messages come
 * from the browser, and each engine words it differently; the strings below are the ones actually seen, not
 * invented.
 */
import { describe, expect, it } from "vitest";
import { isStaleChunkError, STALE_CHUNK_MESSAGE } from "../staleChunk";

describe("isStaleChunkError", () => {
  it("recognises the message Chromium gives for a chunk that is gone", () => {
    // Verbatim from the failure that prompted this: navigating back into the room after the binary was rebuilt.
    expect(
      isStaleChunkError(
        new TypeError(
          "Failed to fetch dynamically imported module: http://127.0.0.1:8080/assets/UniverseScene-rl4xgPwq.js",
        ),
      ),
    ).toBe(true);
  });

  it("recognises the other engines' wordings", () => {
    for (const message of [
      "error loading dynamically imported module",
      "Importing a module script failed.",
      'Unable to preload CSS for /assets/index.v2-abc.css',
    ]) {
      expect(isStaleChunkError(new Error(message)), message).toBe(true);
    }
  });

  it("does not claim a stale build for an ordinary failure", () => {
    // A browser without WebGL is *not* a reload problem, and telling the reader to reload would waste their time
    // on every attempt.
    expect(isStaleChunkError(new Error("this browser could not create a WebGL context"))).toBe(false);
    expect(isStaleChunkError(new Error("Failed to fetch"))).toBe(false);
    expect(isStaleChunkError(undefined)).toBe(false);
    expect(isStaleChunkError("some string")).toBe(false);
  });

  it("says what to do, and blames the page rather than the bundle", () => {
    // The failure is never the bundle's fault: the bytes this page asks for are a build that no longer exists.
    expect(STALE_CHUNK_MESSAGE).toContain("earlier build");
    expect(STALE_CHUNK_MESSAGE).toContain("reload");
    expect(STALE_CHUNK_MESSAGE).toContain("Nothing is wrong with the bundle");
  });
});
