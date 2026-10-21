import Link from "next/link";
import { ArrowRight, GitFork, Star } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { DemoMedia } from "@/components/demo-media";
import { HeroDemoPanel } from "@/components/hero-demo-panel";
import { TerminalBlock } from "@/components/terminal";
import { GITHUB_REPO } from "@/lib/utils";

/** 3×3 capability matrix — rows = depth, cells = differentiators vs grep-only tooling. */
const capabilityRows = [
  {
    label: "Index & ask",
    blurb: "Agent-ready facts without dumping trees into context.",
    cells: [
      {
        title: "discover",
        body: "One Rust indexing pass builds the graph plus reachability caches. Later questions are lookups — not greps.",
      },
      {
        title: "find · callers",
        body: "Exact inventory and typed edges (find, callers, relations, inventory). Stable -f json with schema_version.",
      },
      {
        title: "semantic",
        body: 'Opt-in intent search ("checkout flow") over functions or communities — offline hash/vocab or ONNX, not a substitute for structure.',
      },
    ],
  },
  {
    label: "Program analysis",
    blurb: "Beyond call lists — CFG, PDG, flows, and security paths.",
    cells: [
      {
        title: "blast-radius",
        body: "Deterministic upstream impact for a symbol at any depth — compact JSON agents can gate edits on.",
      },
      {
        title: "cpg · slice · inspect",
        body: "Hybrid CALL + CFG/PDG façade: statement slices, field mutations, dominance — Joern-class depth without leaving the CLI.",
      },
      {
        title: "taint · security",
        body: "Source→sink and CVE-oriented paths when you discover with --with-taint / --with-security — not just “who calls whom.”",
      },
    ],
  },
  {
    label: "Architecture & change",
    blurb: "Where complexity concentrates — and what to migrate first.",
    cells: [
      {
        title: "communities",
        body: "Label-propagation clusters so agents reason about subsystems, not a flat bag of functions.",
      },
      {
        title: "metrics",
        body: "PageRank, betweenness, harmonic centrality, and hotspots — ranked facts, not LLM guesses about importance.",
      },
      {
        title: "migration · check",
        body: "Export a prioritized migration_plan.json; enforce blast-radius policy in CI with check --policy-file.",
      },
    ],
  },
] as const;

export default function HomePage() {
  return (
    <>
      <section className="border-b border-[var(--hairline)]">
        <div className="mx-auto max-w-6xl px-4 pb-10 pt-14 sm:px-6 sm:pb-12 sm:pt-20">
          <Badge className="mb-5">Open source · MIT · Rust</Badge>
          <h1 className="max-w-[21ch] font-[family-name:var(--font-serif)] text-[clamp(1.875rem,4.2vw,2.625rem)] font-semibold leading-[1.18] tracking-[-0.015em] text-[var(--ink)]">
            A code knowledge graph built for agents
          </h1>
          <p className="mt-5 max-w-[58ch] text-[17.5px] leading-relaxed text-[var(--body)]">
            rgctl indexes your repository once, then answers reachability
            and structure questions in compact JSON — so coding agents use{" "}
            <b className="font-semibold text-[var(--ink)]">fewer tokens</b> and
            make{" "}
            <b className="font-semibold text-[var(--ink)]">
              fewer confident mistakes
            </b>
            .
          </p>
          <TerminalBlock
            className="mt-8 max-w-3xl"
            lines={[
              "rgctl --version",
              "rgctl install --skill --tools cursor,claude,codex,antigravity,agents",
            ]}
          />
          <div className="mt-5 flex flex-wrap items-center gap-3">
            <Button size="lg" asChild>
              <Link href="/install/">
                Install <ArrowRight className="h-4 w-4" />
              </Link>
            </Button>
            <Button variant="ghost" size="lg" asChild>
              <a href={GITHUB_REPO} target="_blank" rel="noreferrer">
                <Star className="h-4 w-4" /> Star on GitHub
              </a>
            </Button>
            <Button variant="link" asChild>
              <Link href="/demo/">Watch demos →</Link>
            </Button>
          </div>

          <div className="mt-12">
            <HeroDemoPanel />
          </div>
        </div>
      </section>

      <section className="border-b border-[var(--hairline)]">
        <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6 sm:py-16">
          <h2 className="mb-3 font-[family-name:var(--font-serif)] text-2xl font-semibold tracking-tight text-[var(--ink)]">
            Why rgctl instead of grep?
          </h2>
          <p className="mb-8 max-w-[62ch] text-[var(--body)]">
            Agents that scrape the tree burn tokens and still miss transitive
            callers. Index once; answer with compact JSON.
          </p>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[32rem] border-collapse text-left text-sm">
              <thead>
                <tr className="border-b border-[var(--hairline)] text-[var(--mute)]">
                  <th className="py-2 pr-4 font-medium"> </th>
                  <th className="py-2 pr-4 font-medium">Without rgctl</th>
                  <th className="py-2 font-medium">With rgctl</th>
                </tr>
              </thead>
              <tbody className="text-[var(--body)]">
                <tr className="border-b border-[var(--hairline)]">
                  <td className="py-3 pr-4 font-medium text-[var(--ink)]">
                    Tool calls
                  </td>
                  <td className="py-3 pr-4">Many greps + file dumps</td>
                  <td className="py-3">1–2 (`discover` once, then query)</td>
                </tr>
                <tr className="border-b border-[var(--hairline)]">
                  <td className="py-3 pr-4 font-medium text-[var(--ink)]">
                    Context tokens
                  </td>
                  <td className="py-3 pr-4">Whole files into the prompt</td>
                  <td className="py-3">Compact `-f json` rows</td>
                </tr>
                <tr>
                  <td className="py-3 pr-4 font-medium text-[var(--ink)]">
                    Reachability
                  </td>
                  <td className="py-3 pr-4">Easy to miss / invent</td>
                  <td className="py-3">Deterministic typed graph + blast-radius</td>
                </tr>
              </tbody>
            </table>
          </div>
          <div className="mt-10">
            <h3 className="mb-3 font-[family-name:var(--font-serif)] text-lg font-semibold text-[var(--ink)]">
              Battle-tested at scale
            </h3>
            <ul className="grid gap-3 text-sm text-[var(--body)] sm:grid-cols-2 lg:grid-cols-3">
              <li className="rounded-[4px] border border-[var(--hairline)] p-4">
                Linux kernel · Kubernetes · LLVM · Roslyn · rustc cold discover
              </li>
              <li className="rounded-[4px] border border-[var(--hairline)] p-4">
                Zero-hydration mmap queries on columnar snapshots
              </li>
              <li className="rounded-[4px] border border-[var(--hairline)] p-4">
                14 Tier-1 tree-sitter languages + markdown
              </li>
            </ul>
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-4 py-14 sm:px-6 sm:py-16">
        <h2 className="mb-3 font-[family-name:var(--font-serif)] text-2xl font-semibold tracking-tight text-[var(--ink)]">
          What sets it apart
        </h2>
        <p className="mb-10 max-w-[62ch] text-[var(--body)]">
          Most code graphs stop at symbols, callers, and impact. rgctl adds
          precomputed reachability, hybrid CPG depth, centrality, communities,
          and migration/CI outputs — always as deterministic{" "}
          <code className="font-[family-name:var(--font-mono)] text-[0.9em] text-[var(--ink)]">
            -f json
          </code>{" "}
          for agents.
        </p>
        <div className="space-y-10">
          {capabilityRows.map((row) => (
            <div key={row.label}>
              <div className="mb-4 flex flex-wrap items-baseline gap-x-3 gap-y-1">
                <h3 className="font-[family-name:var(--font-serif)] text-lg font-semibold text-[var(--ink)]">
                  {row.label}
                </h3>
                <p className="text-sm text-[var(--mute)]">{row.blurb}</p>
              </div>
              <div className="grid gap-6 border-t border-[var(--hairline)] pt-5 md:grid-cols-3">
                {row.cells.map((cell) => (
                  <div key={cell.title} className="space-y-2">
                    <h4 className="font-[family-name:var(--font-mono)] text-sm text-[var(--primary)]">
                      {cell.title}
                    </h4>
                    <p className="text-[15px] leading-relaxed text-[var(--body)]">
                      {cell.body}
                    </p>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="border-y border-[var(--hairline)] bg-[var(--canvas-soft)]/60">
        <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6 sm:py-16">
          <h2 className="mb-3 font-[family-name:var(--font-serif)] text-2xl font-semibold tracking-tight text-[var(--ink)]">
            See it run
          </h2>
          <p className="mb-8 max-w-[62ch] text-[var(--body)]">
            The same path as the first hour of the user guide, on the
            ecommerce-java fixture.
          </p>
          <div className="max-w-4xl">
            <DemoMedia
              kind="cli"
              preferGif
              caption="CLI walkthrough — discover, query, blast-radius, semantic search."
            />
          </div>
        </div>
      </section>

      <section className="border-b border-[var(--hairline)]">
        <div className="mx-auto grid max-w-6xl gap-10 px-4 py-14 sm:px-6 sm:py-16 lg:grid-cols-2 lg:items-center">
          <div className="space-y-4">
            <h2 className="font-[family-name:var(--font-serif)] text-2xl font-semibold tracking-tight text-[var(--ink)] sm:text-3xl">
              From prompt → graph facts → edit
            </h2>
            <p className="text-[var(--body)]">
              Install the agent pack (
              <code className="font-mono text-sm">
                rgctl install --skill
              </code>
              ) for a single skill named{" "}
              <code className="font-mono text-sm">rgctl</code> —
              or drop{" "}
              <Link
                href="/agents/"
                className="font-medium text-[var(--primary)] underline"
              >
                AGENTS.md
              </Link>{" "}
              into your repo. The model calls rgctl instead of grepping
              blindly — then reasons on structured impact.
            </p>
            <Button variant="ghost" asChild>
              <Link href="/agents/">
                Agent guide <ArrowRight className="h-4 w-4" />
              </Link>
            </Button>
          </div>
          <TerminalBlock
            lines={[
              "cd your-repo",
              "rgctl install --skill --tools cursor",
              "rgctl discover .",
              'rgctl -f json blast-radius "priceShoppingCart" --depth 2',
            ]}
          />
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-4 py-14 sm:px-6 sm:py-16">
        <div className="flex flex-col gap-6 rounded-lg border border-[var(--hairline)] bg-[var(--surface)] p-8 sm:flex-row sm:items-center sm:justify-between">
          <div className="space-y-2">
            <h2 className="font-[family-name:var(--font-serif)] text-xl font-semibold text-[var(--ink)]">
              Help grow the project
            </h2>
            <p className="max-w-lg text-sm text-[var(--body)]">
              Star the repo, try the ecommerce-java fixture, open issues, and
              share agent recipes. Adoption is the product.
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button asChild>
              <a href={GITHUB_REPO} target="_blank" rel="noreferrer">
                <Star className="h-4 w-4" /> Star
              </a>
            </Button>
            <Button variant="ghost" asChild>
              <a href={`${GITHUB_REPO}/fork`} target="_blank" rel="noreferrer">
                <GitFork className="h-4 w-4" /> Fork
              </a>
            </Button>
            <Button variant="ghost" asChild>
              <Link href="/community/">Community</Link>
            </Button>
          </div>
        </div>
      </section>
    </>
  );
}
