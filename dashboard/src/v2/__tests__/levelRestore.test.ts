/**
 * When the URL should reopen a level — and, more importantly, when it must not.
 *
 * The bug this pins: pressing back inside a level cleared the level in state and in the URL, those two land in
 * different ticks, and for one render the room had `expanded === null` while `?explode=` still named the level it
 * had just closed. The restore effect saw a level that was not open, reopened it, and the reader got exactly what
 * was reported — the universe flashing up and then going away again.
 */
import { describe, expect, it } from "vitest";
import { shouldRestoreLevel } from "../rooms/levelRestore";

const base = { explode: 6, expandedId: null, attemptedId: null, justClosedId: null };

describe("shouldRestoreLevel", () => {
  it("restores a level the URL names and nothing is open", () => {
    expect(shouldRestoreLevel(base)).toBe(true);
  });

  it("does nothing when that level is already open", () => {
    expect(shouldRestoreLevel({ ...base, expandedId: 6 })).toBe(false);
  });

  it("does not reopen the level the reader just closed", () => {
    // The reported defect, as a single call: URL still says 6, state says closed, and 6 was closed by hand.
    expect(shouldRestoreLevel({ ...base, justClosedId: 6 })).toBe(false);
  });

  it("still restores a *different* level after a close", () => {
    // Closing 6 and then following a link to 9 is a real request and must not be suppressed by the guard.
    expect(shouldRestoreLevel({ ...base, explode: 9, justClosedId: 6 })).toBe(true);
  });

  it("does not retry a level a previous attempt already acted on", () => {
    // A stale link whose district is not in the bundle fails once; retrying it on every render would spam the
    // typed absence and, worse, re-run the loader.
    expect(shouldRestoreLevel({ ...base, attemptedId: 6 })).toBe(false);
  });

  it("restores nothing when the URL names no level", () => {
    expect(shouldRestoreLevel({ ...base, explode: null })).toBe(false);
    expect(shouldRestoreLevel({ ...base, explode: undefined })).toBe(false);
  });
});
