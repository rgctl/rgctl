import type { Metadata } from "next";
import Link from "next/link";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { GITHUB_DISCUSSIONS, GITHUB_REPO, GITHUB_RELEASES } from "@/lib/utils";

export const metadata: Metadata = {
  title: "Community",
  description:
    "Contribute to rgctl — language plugins, taint/Kantra rules, agent host adapters, and graph/CPG algorithms.",
  alternates: { canonical: "/community/" },
};

const actions = [
  {
    title: "Star & watch",
    body: "Stars help others discover the project. Watching releases keeps you on CLI updates.",
    href: GITHUB_REPO,
    cta: "Open repository",
  },
  {
    title: "Discussions",
    body: "Ask questions, share agent recipes, and propose docs improvements.",
    href: GITHUB_DISCUSSIONS,
    cta: "Join discussions",
  },
  {
    title: "Issues & PRs",
    body: "Bug reports, language coverage, and docs PRs are the highest-leverage contributions.",
    href: `${GITHUB_REPO}/issues`,
    cta: "Browse issues",
  },
  {
    title: "Releases",
    body: "Grab binaries and read release notes. Docs match the CLI — verify with rgctl --version.",
    href: GITHUB_RELEASES,
    cta: "Latest release",
  },
];

const tracks = [
  {
    title: "Language Plugins & AST Coverage",
    body: "Extend tree-sitter extraction, bump grammars, keep `*-ast-coverage.json` honest.",
    links: [
      { href: "/docs/contributor-checklist/", label: "Contributor checklist" },
      { href: "/docs/tier-1-language-support/", label: "Tier 1 language support" },
      { href: "/docs/language-testing-guide/", label: "Language testing guide" },
      { href: "/docs/languages/", label: "Languages" },
    ],
  },
  {
    title: "Declarative Taint & Kantra Rules",
    body: "Source→sink rules, Kantra integration, and security/OSV workflows.",
    links: [
      { href: "/docs/guides/vulnerability-and-osv-analysis/", label: "OSV / vuln guide" },
      { href: "/docs/analysis-architecture/", label: "Analysis architecture" },
      { href: "/docs/policy-format/", label: "Policy format" },
    ],
  },
  {
    title: "Agent Host Adapters & Skills",
    body: "Pack skills for Cursor/Claude/Codex, keep dogfood `.agents/skills/` in sync.",
    links: [
      { href: "/docs/guides/agent-skill/", label: "Agent pack guide" },
      { href: "/agents/", label: "Agents page" },
      {
        href: `${GITHUB_REPO}/blob/main/skills/rgctl/SKILL.md`,
        label: "skills/rgctl",
        external: true,
      },
    ],
  },
  {
    title: "Graph & CPG Algorithms",
    body: "CFG/PDG, blast-radius, communities, metrics, snapshot storage.",
    links: [
      { href: "/docs/analysis-architecture/", label: "Analysis architecture" },
      { href: "/docs/graph-storage-architecture/", label: "Graph storage" },
      { href: "/docs/guides/hybrid-cpg/", label: "Hybrid CPG guide" },
      { href: "/docs/guides/blast-radius-analysis/", label: "Blast radius guide" },
    ],
  },
];

export default function CommunityPage() {
  return (
    <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6">
      <Badge className="mb-4">Open source</Badge>
      <h1 className="text-3xl tracking-tight text-[var(--ink)] sm:text-4xl">
        Grow rgctl with us
      </h1>
      <p className="mt-3 text-[var(--body)]">
        No product tiers — just an MIT-licensed tool. Adoption means stars,
        forks, working agent integrations, and clear docs.
      </p>

      <section className="mt-12">
        <h2 className="font-[family-name:var(--font-serif)] text-xl font-semibold text-[var(--ink)]">
          Contributor tracks
        </h2>
        <p className="mt-2 max-w-2xl text-sm text-[var(--mute)]">
          Pick a track and jump to the checklist or architecture docs. Full
          setup:{" "}
          <a
            href={`${GITHUB_REPO}/blob/main/CONTRIBUTING.md`}
            className="underline"
            target="_blank"
            rel="noreferrer"
          >
            CONTRIBUTING.md
          </a>
          .
        </p>
        <div className="mt-6 grid gap-4 sm:grid-cols-2">
          {tracks.map((t) => (
            <div
              key={t.title}
              className="rounded-[4px] border border-[var(--hairline)] p-5"
            >
              <h3 className="text-base font-medium text-[var(--ink)]">{t.title}</h3>
              <p className="mt-1 text-sm text-[var(--body)]">{t.body}</p>
              <ul className="mt-3 space-y-1 text-sm">
                {t.links.map((l) => (
                  <li key={l.href}>
                    {"external" in l && l.external ? (
                      <a
                        href={l.href}
                        className="underline"
                        target="_blank"
                        rel="noreferrer"
                      >
                        {l.label}
                      </a>
                    ) : (
                      <Link href={l.href} className="underline">
                        {l.label}
                      </Link>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>
      </section>

      <div className="mt-12 space-y-4">
        {actions.map((a) => (
          <div
            key={a.title}
            className="rounded-[4px] border border-[var(--hairline)] p-5"
          >
            <h2 className="text-base font-medium text-[var(--ink)]">
              {a.title}
            </h2>
            <p className="mt-1 text-sm text-[var(--body)]">{a.body}</p>
            <Button variant="ghost" size="sm" className="mt-3" asChild>
              <a href={a.href} target="_blank" rel="noreferrer">
                {a.cta}
              </a>
            </Button>
          </div>
        ))}
      </div>
    </div>
  );
}
