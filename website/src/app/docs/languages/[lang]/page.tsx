import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { Badge } from "@/components/ui/badge";
import {
  formatExtensions,
  getLanguage,
  groupHandlers,
  HANDLER_ORDER,
  listLanguages,
  loadCoverage,
} from "@/lib/languages";
import { GITHUB_REPO } from "@/lib/utils";

type Props = { params: Promise<{ lang: string }> };

export function generateStaticParams() {
  return listLanguages().map((l) => ({ lang: l.id }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { lang } = await params;
  const entry = getLanguage(lang);
  const name = entry?.displayName ?? "Language";
  const title = `${name} Code Knowledge Graph, Call Graph & AST Coverage`;
  const description = `${name} support in rgctl: tree-sitter AST coverage, call graph, CFG/PDG, and code knowledge graph queries for LLM agents.`;
  const canonical = `/docs/languages/${lang}/`;
  return {
    title,
    description,
    alternates: { canonical },
    openGraph: {
      title,
      description,
      type: "article",
      url: canonical,
    },
  };
}

export default async function LanguageSupportPage({ params }: Props) {
  const { lang: id } = await params;
  const entry = getLanguage(id);
  const coverage = loadCoverage(id);
  if (!entry || !coverage) notFound();

  const groups = groupHandlers(coverage.handlers);
  const manifestPath = `crates/${entry.crateDir}/${entry.manifestFile}`;
  const githubManifest = `${GITHUB_REPO}/blob/main/${manifestPath}`;

  return (
    <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6">
      <p className="mb-6 text-sm text-[var(--mute)]">
        <Link href="/docs/languages/" className="underline">
          Languages
        </Link>
        {` / ${entry.displayName}`}
        {" · "}
        <a
          href={githubManifest}
          className="underline"
          target="_blank"
          rel="noreferrer"
        >
          Edit coverage JSON
        </a>
      </p>

      <Badge className="mb-4">AST coverage</Badge>
      <h1 className="text-3xl tracking-tight text-[var(--ink)] sm:text-4xl">
        {entry.displayName}
      </h1>
      <p className="mt-3 text-[var(--body)]">
        Support matrix rendered from{" "}
        <code className="text-sm">{entry.manifestFile}</code>. Update that file
        when bumping the grammar; the site regenerates on the next build.
      </p>

      <dl className="mt-8 grid gap-3 text-sm sm:grid-cols-2">
        <Meta label="Grammar" value={entry.grammar || "—"} mono />
        <Meta
          label="Plugin"
          value={entry.plugin ? `${entry.plugin} (${entry.crateDir})` : entry.crateDir}
          mono
        />
        <Meta label="Extensions" value={formatExtensions(entry.extensions)} mono />
        <Meta
          label="Discover"
          value={`rgctl discover . -l ${id} --with-cfg`}
          mono
        />
      </dl>

      <h2 className="mt-10 text-xl font-medium text-[var(--ink)]">
        Handler summary
      </h2>
      <div className="mt-4 overflow-x-auto">
        <table className="w-full border-collapse text-sm">
          <thead>
            <tr className="border-b border-[var(--hairline)] text-left text-[var(--mute)]">
              <th className="p-2 font-medium">Handler</th>
              <th className="p-2 font-medium">Kinds</th>
            </tr>
          </thead>
          <tbody>
            {HANDLER_ORDER.map((h) => (
              <tr
                key={h}
                className="border-b border-[var(--hairline)] text-[var(--body)]"
              >
                <td className="p-2 font-mono text-[var(--ink)]">{h}</td>
                <td className="p-2">{groups.get(h)?.length ?? 0}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {HANDLER_ORDER.map((h) => {
        const kinds = groups.get(h) ?? [];
        if (!kinds.length) return null;
        return (
          <section key={h} className="mt-10">
            <h2 className="text-lg font-medium text-[var(--ink)]">
              {h}{" "}
              <span className="text-sm font-normal text-[var(--mute)]">
                ({kinds.length})
              </span>
            </h2>
            <ul className="mt-3 columns-1 gap-x-6 text-sm text-[var(--body)] sm:columns-2">
              {kinds.map((k) => (
                <li key={k} className="break-inside-avoid font-mono text-xs leading-6">
                  {k}
                </li>
              ))}
            </ul>
          </section>
        );
      })}

      <p className="mt-12 text-sm text-[var(--mute)]">
        Related:{" "}
        <Link href="/docs/tier-1-language-support/" className="underline">
          Tier 1 language support
        </Link>
        {" · "}
        <Link href="/docs/guides/discovering-and-indexing/" className="underline">
          Discovering and indexing
        </Link>
      </p>
    </div>
  );
}

function Meta({
  label,
  value,
  mono,
}: {
  label: string;
  value: string;
  mono?: boolean;
}) {
  return (
    <div className="rounded-[4px] border border-[var(--hairline)] p-3">
      <dt className="text-[var(--mute)]">{label}</dt>
      <dd
        className={`mt-1 text-[var(--ink)] ${mono ? "font-mono text-xs break-all" : ""}`}
      >
        {value}
      </dd>
    </div>
  );
}
