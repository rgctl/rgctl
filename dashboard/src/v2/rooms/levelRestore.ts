/**
 * Whether the URL should (re)open a level — including the case where it must not.
 *
 * The level is carried in `?explode=`, so the room restores it on mount. That is the whole point of the URL
 * (leaving the room for a symbol and coming back lands on the level you were reading) and it is also a race: the
 * reader pressing back clears the level in state *and* in the URL, and those two do not land in the same tick. For
 * one render the room has `expanded === null` while the URL still names the level it just closed — and a restore
 * effect that only asks "is the URL's level the open one?" answers *no*, reopens it, and the reader sees the
 * universe flash and then vanish again.
 *
 * So the decision needs the third fact: what was just closed. It is a pure function rather than a condition inside
 * a `useEffect` because this is exactly the kind of ordering bug that is invisible in review and cheap to test.
 */
export function shouldRestoreLevel({
  explode,
  expandedId,
  attemptedId,
  justClosedId,
}: {
  /** The level the URL names, or `null`. */
  explode: number | null | undefined;
  /** The level currently open, or `null`. */
  expandedId: number | null;
  /** The level a previous restore already acted on, so a failure cannot retry on every render. */
  attemptedId: number | null;
  /** The level the reader closed by hand, whose URL write may still be in flight. */
  justClosedId: number | null;
}): boolean {
  if (explode === null || explode === undefined) return false;
  // Already open.
  if (expandedId === explode) return false;
  // The reader just closed this one: the URL has not caught up, and reopening it is the flicker.
  if (justClosedId === explode) return false;
  // Already acted on — a stale link must not be retried forever.
  if (attemptedId === explode) return false;
  return true;
}
