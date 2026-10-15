/**
 * The Review room's ways of saying nothing, and the two things it must never say.
 *
 * Spec §6.2–6.5. Each absence state gets its own test because they are four different facts with four
 * different sentences, and the failure mode this room exists to prevent is collapsing them into one:
 * a `policy: null` rendered as "no violations" is a lie a review surface tells very easily, and an
 * artifact whose summary overstates a change is worse than one with no summary at all.
 *
 * These render against a **real** artifact (`fixtures/review-artifact.json`, a one-line deletion whose
 * correct reading is `added: 0, removed: 8`) rather than a hand-written minimal one, so a change in the
 * producer's shape shows up here instead of being masked by a fixture that agreed with the old shape.
 */
import { render, screen, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { afterEach, expect, it, vi } from "vitest";
import { createAppRouter } from "../router";
// The room's own source, as text. Vite's `?raw` rather than `node:fs`: `tsconfig.v2.json` declares only
// `vite/client` types, and pulling in Node's declarations to read one file is a poor trade.
import roomSource from "../rooms/Review.tsx?raw";
import {
  installFetchStub,
  REVIEW_ARTIFACT,
  REVIEW_ARTIFACT_UNRESOLVED,
  REVIEW_HEAD_DIGEST,
  reviewFunctions,
} from "./fixture";
import type { ReviewArtifact } from "../workflow";

afterEach(() => vi.unstubAllGlobals());

function renderRoom() {
  const history = createMemoryHistory({ initialEntries: ["/review"] });
  const router = createAppRouter(history);
  render(
    <QueryClientProvider
      client={
        new QueryClient({
          defaultOptions: { queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false } },
        })
      }
    >
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

function panel(title: string): HTMLElement {
  const heading = screen.getByText(title, { selector: "h2" });
  const section = heading.closest("section");
  if (!section) throw new Error(`no panel titled ${title}`);
  return section;
}

/** The rendered row for a changed symbol, found by its own name rather than by position. */
function symbolRow(name: string): HTMLElement {
  const rows = Array.from(panel("Changed symbols").querySelectorAll<HTMLElement>("ul > li"));
  const row = rows.find((r) => r.firstElementChild?.textContent?.startsWith(name));
  if (!row) throw new Error(`no symbol row for ${name}`);
  return row;
}

/** A `Stat` shows its value in the sibling of its label — asserted as a pair, never as a loose number. */
function statShows(label: string, value: string): boolean {
  return screen
    .getAllByText(label)
    .some((el) => el.parentElement?.textContent === `${label}${value}`);
}

// --- the absent artifact -----------------------------------------------------

it("says there is no artifact, and gives the command that makes one", async () => {
  installFetchStub(); // no `review` option: the bundle carries no artifact
  renderRoom();

  expect(await screen.findByText("no artifact in this bundle")).toBeTruthy();
  expect(screen.getByText(/No review artifact was exported with this dashboard/)).toBeTruthy();
  // The advertised command has to be one that works: the export refuses to synthesize a head snapshot.
  expect(screen.getByText(/rgctl review export --full-snapshots/)).toBeTruthy();
});

it("reports an artifact that is present but unreadable as broken, not as missing", async () => {
  installFetchStub({ review: "{ this is not json" });
  renderRoom();

  expect(await screen.findByText("unreadable")).toBeTruthy();
  expect(screen.getByText(/is not valid JSON/)).toBeTruthy();
  expect(screen.queryByText("no artifact in this bundle")).toBeNull();
});

// --- what the artifact actually says -----------------------------------------

it("renders a deletion as removals, not as additions", async () => {
  // The regression this pins: with a synthesized head the same one-line deletion read as
  // `added: 22, removed: 0`. See §8 of the dashboard-v2 change.
  installFetchStub({
    review: REVIEW_ARTIFACT,
    manifest: { graph: { node_count: 42, edge_count: 7, digest: REVIEW_HEAD_DIGEST } },
  });
  renderRoom();

  await screen.findByText("changed symbols");
  expect(statShows("removed", "8")).toBe(true);
  expect(statShows("call edges added", "0")).toBe(true);

  // And the deltas read as removals rather than merely being counted as such. Asserted against the row's
  // text rather than a text matcher: the arrow is a nested `<span>`, so no single element's *direct* text
  // is the whole delta.
  const removal = REVIEW_ARTIFACT.review_paths!.symbols!
    .flatMap((s) => (s.path_delta ?? []).map((d) => ({ ...d, symbol: s.name ?? "" })))
    .find((d) => d.kind === "removed");
  expect(removal, "the fixture artifact carries a removal").toBeTruthy();
  const row = symbolRow(removal!.symbol);
  expect(row.textContent).toContain(`${removal!.from} → ${removal!.to}`);
  expect(row.textContent).toContain("removed");
});

it("does not let an unevaluated policy read as a pass", async () => {
  installFetchStub({ review: REVIEW_ARTIFACT });
  renderRoom();

  expect(await screen.findByText("Not evaluated")).toBeTruthy();
  expect(screen.getByText(/no --policy-file provided; policy was not evaluated/)).toBeTruthy();
  // The lie this guards against, stated as an assertion.
  expect(screen.queryByText(/^pass$/i)).toBeNull();
  expect(screen.queryByText(/no violations found/i)).toBeNull();
});

it("says why a section is missing even when the artifact recorded no reason", async () => {
  const bare: ReviewArtifact = {
    ...REVIEW_ARTIFACT,
    provenance: { ...REVIEW_ARTIFACT.provenance, unavailable: {} },
  };
  installFetchStub({ review: bare });
  renderRoom();

  expect(await screen.findByText(/the artifact records that no policy was evaluated/)).toBeTruthy();
  expect(screen.getByText(/the artifact records an empty timeline/)).toBeTruthy();
});

// --- truncation (§6.5) --------------------------------------------------------

it("cannot show a truncated report without saying so — even when only one symbol was cut", async () => {
  const symbolCut: ReviewArtifact = {
    ...REVIEW_ARTIFACT,
    truncation: { symbols: false, fanout: false, depth: false },
    review_paths: {
      ...REVIEW_ARTIFACT.review_paths,
      truncation: { symbols: false, fanout: false, depth: false },
      symbols: (REVIEW_ARTIFACT.review_paths?.symbols ?? []).map((s, i) =>
        i === 0 ? { ...s, truncation: { fanout: true } } : { ...s, truncation: {} },
      ),
    },
  };
  installFetchStub({ review: symbolCut });
  renderRoom();

  expect(await screen.findByText(/The report was truncated \(fanout\)/)).toBeTruthy();
});

it("stays quiet when nothing was cut", async () => {
  const whole: ReviewArtifact = {
    ...REVIEW_ARTIFACT,
    truncation: { symbols: false, fanout: false, depth: false },
    review_paths: {
      ...REVIEW_ARTIFACT.review_paths,
      truncation: { symbols: false, fanout: false, depth: false },
      symbols: (REVIEW_ARTIFACT.review_paths?.symbols ?? []).map((s) => ({ ...s, truncation: {} })),
    },
  };
  installFetchStub({ review: whole });
  renderRoom();

  await screen.findByText("changed symbols");
  expect(screen.queryByText(/was truncated/)).toBeNull();
});

// --- freshness (§6.3) ---------------------------------------------------------

it("states a digest mismatch above the figures rather than presenting them as current", async () => {
  installFetchStub({ review: REVIEW_ARTIFACT }); // manifest digest stays "deadbeef…"
  renderRoom();

  expect(await screen.findByText(/describes a different revision/)).toBeTruthy();
  expect(screen.getByText("other revision")).toBeTruthy();
  expect(screen.queryByText("same revision")).toBeNull();
});

it("claims sameness only when the digests actually agree", async () => {
  installFetchStub({
    review: REVIEW_ARTIFACT,
    manifest: { graph: { node_count: 42, edge_count: 7, digest: REVIEW_HEAD_DIGEST } },
  });
  renderRoom();

  expect(await screen.findByText("same revision")).toBeTruthy();
  expect(screen.queryByText(/describes a different revision/)).toBeNull();
});

// --- identity (§6.4) ----------------------------------------------------------

it("refuses to hand off an ambiguous name, and names the ambiguity", async () => {
  const [subject] = reviewFunctions();
  installFetchStub({
    review: REVIEW_ARTIFACT,
    // A second indexed function with the same name in the same file.
    extraFunctions: [{ ...subject, function_id: "dddddddd-0000-4000-8000-000000000002" }],
  });
  renderRoom();

  await screen.findByText(subject.name);
  const row = symbolRow(subject.name);
  expect(within(row).getByText(/ambiguous \(2\)/)).toBeTruthy();
  expect(within(row).queryByText("carry →")).toBeNull();
});

it("carries a uniquely identified symbol, and says when a symbol is not in this graph", async () => {
  const absent = reviewFunctions()[4].name;
  installFetchStub({ review: REVIEW_ARTIFACT, omitReviewFunctions: [absent] });
  renderRoom();

  await screen.findByText(absent);
  const missing = symbolRow(absent);
  expect(within(missing).getByText("not in this graph")).toBeTruthy();
  expect(within(missing).queryByText("carry →")).toBeNull();

  // A symbol that does resolve offers the handoff, and it resolves to a real index id.
  const present = reviewFunctions()[0];
  const row = symbolRow(present.name);
  expect(within(row).getByText("carry →")).toBeTruthy();
});

// --- a scoped export that resolved to nothing (§5.5) --------------------------

it("refuses to present a symbol-scoped artifact that did not resolve as a review", async () => {
  installFetchStub({ review: REVIEW_ARTIFACT_UNRESOLVED });
  renderRoom();

  expect(await screen.findByText("no symbol resolved")).toBeTruthy();
  expect(screen.getByText(/matched 3 functions/)).toBeTruthy();

  // The document's figures are all zero. Rendering them would read as "nothing changed", which is the
  // lie this state exists to prevent — so the summary row and the symbol table are both absent.
  expect(screen.queryByText("changed symbols")).toBeNull();
  expect(screen.queryByText("call edges added")).toBeNull();
  expect(screen.queryByText("Changed symbols")).toBeNull();

  // And the candidates are listed, so the reader can act on the ambiguity instead of guessing.
  expect(screen.getAllByText("checkout")).toHaveLength(3);
  expect(screen.getByText(/Re-export without/)).toBeTruthy();
});

it("shows the unscored-file count when the artifact records one", async () => {
  // `unscored_files` was 0 in every fixture, so this path had never been rendered. The mutation is a
  // *state*, not a shape: the artifact is the real export with one field set, which is the same technique
  // the truncation tests use.
  installFetchStub({
    review: {
      ...REVIEW_ARTIFACT,
      review_paths: {
        ...REVIEW_ARTIFACT.review_paths,
        change_summary: { ...REVIEW_ARTIFACT.review_paths!.change_summary, unscored_files: 2 },
      },
    },
  });
  renderRoom();

  await screen.findByText("changed symbols");
  expect(screen.getByText("unscored files")).toBeTruthy();
  expect(statShows("unscored files", "2")).toBe(true);
});

// --- the contract stays in the adapter (§2.4) ---------------------------------

it("keeps the artifact's field names inside the adapter", async () => {
  // §2.4 asked for this to be asserted in review: "grep for field names outside the adapter should return
  // nothing". A test does it on every run, and this is the assertion that keeps the room rendering a view
  // model instead of the artifact's shape.
  const room = roomSource;
  const artifactFields = [
    "review_paths",
    "change_summary",
    "path_delta",
    "path_before",
    "path_after",
    "stable_key",
    "call_edges",
    "unscored_files",
    "generated_at",
    "provenance",
  ];
  expect(artifactFields.filter((field) => room.includes(field))).toEqual([]);
});

