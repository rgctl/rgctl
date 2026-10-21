import type { Metadata } from "next";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";

export const metadata: Metadata = {
  title: "Guides",
  description:
    "Task-oriented rgctl learning paths — discover, impact analysis, CFG/PDG, OSV reachability, and migration — with ecommerce-java and CoolStore fixtures.",
  alternates: { canonical: "/docs/guides/" },
};

type Guide = {
  title: string;
  feature: string;
  blurb: string;
  href: string;
};

type Path = {
  title: string;
  blurb: string;
  guides: Guide[];
};

const paths: Path[] = [
  {
    title: "Start Here & Agent Setup",
    blurb: "Index a repo, install the agent pack, and learn structured queries.",
    guides: [
      {
        title: "Discovering and indexing",
        feature: "discover",
        blurb: "Build the knowledge graph from source code.",
        href: "/docs/guides/discovering-and-indexing/",
      },
      {
        title: "Agent pack",
        feature: "install --skill",
        blurb: "Single skill install; NL → CLI via references/workflows.md.",
        href: "/docs/guides/agent-skill/",
      },
      {
        title: "Structured graph queries",
        feature: "find / callers / relations / …",
        blurb: "Agent-facing graph exploration verbs.",
        href: "/docs/guides/structured-query/",
      },
      {
        title: "Watch mode and incremental update",
        feature: "update, serve --watch",
        blurb: "Keep the graph fresh without full rediscover.",
        href: "/docs/guides/watch-mode/",
      },
    ],
  },
  {
    title: "Impact, PR Review & CI Gates",
    blurb: "Measure change impact and gate merges with policy.",
    guides: [
      {
        title: "Blast radius analysis",
        feature: "blast-radius",
        blurb: "Measure upstream impact before changing a function.",
        href: "/docs/guides/blast-radius-analysis/",
      },
      {
        title: "Pull request review",
        feature: "review paths, review check",
        blurb: "Structural before/after call-path inspection and PR gates.",
        href: "/docs/guides/pr-review/",
      },
      {
        title: "CI policy checks",
        feature: "check, pr-check",
        blurb: "PR temporal gates and local policy checks.",
        href: "/docs/guides/ci-policy-checks/",
      },
    ],
  },
  {
    title: "Deep Code Analysis",
    blurb: "CFG, PDG, slicing, hybrid CPG, and clone detection.",
    guides: [
      {
        title: "Program slicing",
        feature: "slice",
        blurb: "Extract the minimal set of statements affecting a variable.",
        href: "/docs/guides/program-slicing/",
      },
      {
        title: "Hybrid CPG",
        feature: "cpg",
        blurb: "Combined call-graph + per-function CFG/PDG analysis.",
        href: "/docs/guides/hybrid-cpg/",
      },
      {
        title: "Inspecting CFG, PDG, and dominance",
        feature: "inspect",
        blurb: "Examine control flow, data dependence, and dominator trees.",
        href: "/docs/guides/inspecting-cfg-pdg-dominance/",
      },
      {
        title: "Clone detection",
        feature: "clones",
        blurb: "Exact (Type-1), bloom, and sub-function fragment clones.",
        href: "/docs/guides/clone-detection/",
      },
    ],
  },
  {
    title: "Security & Supply-Chain Reachability",
    blurb: "OSV triage, deps scanning, and call-path exploitability.",
    guides: [
      {
        title: "Vulnerability and OSV supply-chain analysis",
        feature: "vuln, deps, taint, security",
        blurb:
          "OSV advisory triage, manifest scanning, call-path reachability, and OpenVEX.",
        href: "/docs/guides/vulnerability-and-osv-analysis/",
      },
    ],
  },
  {
    title: "Architecture, Search & Migration",
    blurb: "Communities, metrics, semantic search, migration, docs graph, export, serve.",
    guides: [
      {
        title: "Community detection",
        feature: "communities",
        blurb: "Identify and label functional clusters in your codebase.",
        href: "/docs/guides/community-detection/",
      },
      {
        title: "Graph metrics",
        feature: "metrics",
        blurb: "PageRank, betweenness, and community detection analytics.",
        href: "/docs/guides/graph-metrics/",
      },
      {
        title: "Semantic search",
        feature: "semantic",
        blurb: "Natural-language search over function symbols.",
        href: "/docs/guides/semantic-search/",
      },
      {
        title: "Migration planning",
        feature: "discover --export-migration-hints",
        blurb: "Generate a dependency-aware migration roadmap.",
        href: "/docs/guides/migration-planning/",
      },
      {
        title: "Markdown context graph",
        feature: "discover -l markdown · export",
        blurb: "Index docs, Obsidian/OKF export, fixture feature tour.",
        href: "/docs/guides/markdown-context-graph/",
      },
      {
        title: "Exporting graphs",
        feature: "export",
        blurb: "Serialize to JSON, GraphML, Graphviz, Mermaid, or Obsidian.",
        href: "/docs/guides/exporting-graphs/",
      },
      {
        title: "HTTP server and dashboard",
        feature: "serve",
        blurb: "Run an HTTP API and browser-based dashboard.",
        href: "/docs/guides/http-server-and-dashboard/",
      },
    ],
  },
];

export default function GuidesIndexPage() {
  return (
    <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6">
      <Badge className="mb-4">Guides</Badge>
      <h1 className="text-3xl tracking-tight text-[var(--ink)] sm:text-4xl">
        Guides
      </h1>
      <p className="mt-3 max-w-2xl text-[var(--body)]">
        Task-oriented learning paths. Pick a path, then open the guide that matches
        the job.
      </p>

      <aside className="mt-8 rounded-[4px] border border-[var(--hairline)] bg-[var(--canvas-soft)]/60 p-5 text-sm text-[var(--body)]">
        <p className="font-medium text-[var(--ink)]">Which fixture should I use?</p>
        <ul className="mt-2 list-disc space-y-1 pl-5">
          <li>
            <strong className="text-[var(--ink)]">In-tree quickstart:</strong>{" "}
            <code className="text-xs">rgctl-tests/ecommerce-java</code> — ships with
            this repo; no extra clone. Ideal for first{" "}
            <code className="text-xs">discover</code> /{" "}
            <code className="text-xs">find</code> /{" "}
            <code className="text-xs">blast-radius</code> runs (also used in the{" "}
            <Link href="/docs/user-guide/" className="underline">
              User Guide
            </Link>
            ).
          </li>
          <li>
            <strong className="text-[var(--ink)]">Full feature guides:</strong>{" "}
            <code className="text-xs">example/coolstore</code> (or{" "}
            <a
              href="https://github.com/konveyor-ecosystem/coolstore"
              className="underline"
              target="_blank"
              rel="noreferrer"
            >
              konveyor-ecosystem/coolstore
            </a>
            ) — larger Java EE app used in the step-by-step guides below.
          </li>
        </ul>
      </aside>

      <div className="mt-12 space-y-12">
        {paths.map((path) => (
          <section key={path.title}>
            <h2 className="font-[family-name:var(--font-serif)] text-xl font-semibold text-[var(--ink)]">
              {path.title}
            </h2>
            <p className="mt-1 max-w-2xl text-sm text-[var(--mute)]">{path.blurb}</p>
            <div className="mt-5 grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {path.guides.map((g) => (
                <Link
                  key={g.href}
                  href={g.href}
                  className="group flex flex-col rounded-[4px] border border-[var(--hairline)] bg-[var(--canvas-soft)]/50 p-5 transition-colors hover:border-[var(--mute)]"
                >
                  <h3 className="text-base font-medium text-[var(--ink)]">{g.title}</h3>
                  <p className="mt-1 font-mono text-[11px] text-[var(--mute)]">
                    {g.feature}
                  </p>
                  <p className="mt-2 text-sm text-[var(--body)]">{g.blurb}</p>
                </Link>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}
