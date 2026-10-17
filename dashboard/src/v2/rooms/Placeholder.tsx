/**
 * Rooms that are changes 2–6.
 *
 * They render an explicit, typed absence naming the change that fills them, rather than an empty panel.
 * A reviewer can then tell "not built yet" from "built and found nothing" — the same distinction the
 * shell enforces everywhere else.
 */
import { Panel, Unavailable } from "../shell";
import type { LensId, RoomId } from "../focus";

const PENDING: Partial<Record<RoomId, { change: string; what: string; why: string }>> = {
  review: {
    change: "change 2 (add-review-workflow-dashboard)",
    what: "Review room",
    why: "Renders the composed artifact from `rgctl review export` — review paths, policy result and the violation timeline in one versioned file, with provenance and truncation recorded so a truncated report cannot be mistaken for a complete one.",
  },
  security: {
    change: "change 3 (add-security-workflow-dashboard)",
    what: "Security room",
    why: "Risk-weighted triage: severity × centrality × reachability, so the list is ordered by what actually matters rather than by rule id.",
  },
  architecture: {
    change: "change 4 (add-architecture-city-view)",
    what: "Architecture room",
    why: "Communities and coupling, with the 3D Code City arriving here at the earliest — depth has to encode real data or it is decoration.",
  },
  migration: {
    change: "change 6 (add-migration-workflow-dashboard)",
    what: "Migration room",
    why: "The deterministic schedule from coupling, centrality and blast radius, restyled onto the v2 shell.",
  },
  proof: {
    change: "change 5 (add-proof-surface)",
    what: "Proof room",
    why: "The speed matrix, correctness scorecard, determinism checks and published limits — the numbers that justify trusting the rest.",
  },
};

/**
 * Lenses that are not written yet, keyed by **lens**.
 *
 * `Placeholder` above is keyed by room, so rendering `<Placeholder room="understand" />` for an unbuilt
 * lens returned `null` — five lens tabs (dataflow, slice, blast, metrics, time) produced an empty pane with
 * nothing saying why. A tab that appears functional and renders nothing is worse than a tab that is
 * visibly absent.
 *
 * Each entry says what the lens will show and why it is not there, and the two reasons are genuinely
 * different — which is the point of writing them out rather than saying "not built yet" five times:
 * most lenses have their data and simply no code, while `metrics` is **blocked by a keying mismatch**.
 */
const PENDING_LENSES: Partial<Record<LensId, { what: string; why: string; status: string; tag: string }>> = {
  dataflow: {
    tag: "code not written",
    what: "Dataflow lens",
    status: "not written yet — the data is here",
    why: "Def-use over the PDG for the focused symbol: which definitions reach its uses, and which of its definitions reach further. This bundle carries the detail (`dataflow_detail_dir: slice`), so the lens is code that has not been written rather than data that is missing.",
  },
  slice: {
    tag: "code not written",
    what: "Slice / Trace lens",
    status: "not written yet — the data is here",
    why: "A statement-level backward or forward slice for the focused symbol — the lines that must run for it, or that it affects. The `slice/` details are in this bundle for every indexed function.",
  },
  blast: {
    tag: "code not written",
    what: "Blast lens",
    status: "not written yet — the data is here",
    why: "What the symbol can reach, SCC-condensed, with the caveat that reachability uses `Calls` edges only — so indirect, virtual and macro call sites under-report impact.",
  },
  metrics: {
    tag: "blocked: no join key",
    what: "Metrics lens",
    status: "blocked, not unbuilt",
    why: "Per-symbol centrality cannot be shown honestly from this bundle. `function_metrics.json` rows are keyed by columnar node `index` (0…2944) while every other artifact here is keyed by `function_id`, so there is no join between them. Printing a zero, or matching by position, would be a fabricated number — the lens says so instead.",
  },
  time: {
    tag: "needs 2 snapshots",
    what: "Time lens",
    status: "not written yet — needs a second snapshot",
    why: "Temporal drift: what this symbol did between two revisions. The mechanism exists — node identity is `BLAKE3(path, name, type)`, so a diff can say new, existing, resolved or regression — but a bundle carries one snapshot, so the lens needs two before it has anything to compare.",
  },
};

export function LensPlaceholder({ lens }: { lens: LensId }) {
  const info = PENDING_LENSES[lens];
  if (!info) return null;
  return (
    <Panel title={info.what} subtitle={info.status}>
      <Unavailable tag={info.tag} what={`${info.what} is not implemented yet`} why={info.why} />
      <p className="mt-3 text-[11px] leading-relaxed text-zinc-500">
        The subject is not lost — the Focus is still selected and every other lens reads the same one. A
        lens that cannot show something says so rather than rendering an empty pane, because an empty pane
        is indistinguishable from "nothing found".
      </p>
    </Panel>
  );
}

export function Placeholder({ room }: { room: RoomId }) {
  const info = PENDING[room];
  if (!info) return null;
  return (
    <Panel title={info.what} subtitle={`not built yet — ${info.change}`}>
      <Unavailable
        tag="not implemented yet"
        what={`${info.what} is not implemented in change 1`}
        why={info.why}
      />
      <p className="mt-3 text-[11px] leading-relaxed text-zinc-500">
        This is deliberate: change 1 builds the frame and the Focus, and adds no analysis view. The rooms
        arrive as changes 2–6 and are expected to be additive — if one of them needs a new surface rather
        than a new panel, the frame is wrong.
      </p>
    </Panel>
  );
}
