import Link from "next/link";

type NavLink = { href: string; label: string };

type NavSection = { title: string; links: NavLink[] };

export const DOCS_NAV: NavSection[] = [
  {
    title: "Start Here & Agent Setup",
    links: [
      { href: "/docs/guides/discovering-and-indexing/", label: "Discovering and indexing" },
      { href: "/docs/guides/agent-skill/", label: "Agent pack" },
      { href: "/docs/guides/structured-query/", label: "Structured graph queries" },
      { href: "/docs/guides/watch-mode/", label: "Watch mode and incremental update" },
    ],
  },
  {
    title: "Impact, PR Review & CI Gates",
    links: [
      { href: "/docs/guides/blast-radius-analysis/", label: "Blast radius analysis" },
      { href: "/docs/guides/pr-review/", label: "Pull request review" },
      { href: "/docs/guides/ci-policy-checks/", label: "CI policy checks" },
    ],
  },
  {
    title: "Deep Code Analysis",
    links: [
      { href: "/docs/guides/program-slicing/", label: "Program slicing" },
      { href: "/docs/guides/hybrid-cpg/", label: "Hybrid CPG" },
      { href: "/docs/guides/inspecting-cfg-pdg-dominance/", label: "Inspect CFG / PDG / dominance" },
      { href: "/docs/guides/clone-detection/", label: "Clone detection" },
    ],
  },
  {
    title: "Security & Supply-Chain",
    links: [
      {
        href: "/docs/guides/vulnerability-and-osv-analysis/",
        label: "Vulnerability and OSV analysis",
      },
    ],
  },
  {
    title: "Architecture, Search & Migration",
    links: [
      { href: "/docs/guides/community-detection/", label: "Community detection" },
      { href: "/docs/guides/graph-metrics/", label: "Graph metrics" },
      { href: "/docs/guides/semantic-search/", label: "Semantic search" },
      { href: "/docs/guides/migration-planning/", label: "Migration planning" },
      { href: "/docs/guides/markdown-context-graph/", label: "Markdown context graph" },
      { href: "/docs/guides/exporting-graphs/", label: "Exporting graphs" },
      { href: "/docs/guides/http-server-and-dashboard/", label: "HTTP server and dashboard" },
    ],
  },
  {
    title: "Reference",
    links: [
      { href: "/docs/user-guide/", label: "User Guide" },
      { href: "/docs/json-api/", label: "JSON API" },
      { href: "/docs/policy-format/", label: "Policy Format" },
      { href: "/docs/languages/", label: "Languages" },
    ],
  },
  {
    title: "Architecture & Contributing",
    links: [
      { href: "/docs/contributor-checklist/", label: "Contributor Checklist" },
      { href: "/docs/tier-1-language-support/", label: "Tier 1 Language Support" },
      { href: "/docs/language-testing-guide/", label: "Language Testing Guide" },
      { href: "/docs/analysis-architecture/", label: "Analysis Architecture" },
      { href: "/docs/graph-storage-architecture/", label: "Graph Storage Architecture" },
    ],
  },
];

export function DocsSidebar({ currentPath }: { currentPath?: string }) {
  return (
    <nav
      aria-label="Documentation"
      className="space-y-6 text-sm lg:sticky lg:top-24 lg:max-h-[calc(100vh-7rem)] lg:overflow-y-auto"
    >
      <p className="font-[family-name:var(--font-serif)] text-base font-semibold text-[var(--ink)]">
        Docs
      </p>
      {DOCS_NAV.map((section) => (
        <div key={section.title}>
          <p className="mb-2 text-xs font-semibold uppercase tracking-wide text-[var(--mute)]">
            {section.title}
          </p>
          <ul className="space-y-1.5">
            {section.links.map((link) => {
              const active =
                currentPath === link.href ||
                (currentPath?.startsWith(link.href) ?? false);
              return (
                <li key={link.href}>
                  <Link
                    href={link.href}
                    className={
                      active
                        ? "font-medium text-[var(--primary)] underline"
                        : "text-[var(--body)] hover:text-[var(--ink)] hover:underline"
                    }
                  >
                    {link.label}
                  </Link>
                </li>
              );
            })}
          </ul>
        </div>
      ))}
    </nav>
  );
}
