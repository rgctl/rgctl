import type { Metadata } from "next";
import Link from "next/link";
import { notFound } from "next/navigation";
import { DocsSidebar } from "@/components/docs-sidebar";
import { DocMarkdown } from "@/components/doc-markdown";
import { extractDocMeta, listDocSlugs, readDoc, rewriteDocLinks } from "@/lib/docs";
import { GITHUB_REPO } from "@/lib/utils";

type Props = { params: Promise<{ slug: string[] }> };

export function generateStaticParams() {
  return listDocSlugs()
    .filter((slug) => !(slug.length === 1 && slug[0] === "guides"))
    .map((slug) => ({ slug }));
}

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const { slug } = await params;
  const raw = readDoc(slug);
  const fallback = slug[slug.length - 1] ?? "docs";
  const meta = extractDocMeta(raw ?? "", fallback);
  const canonicalPath = `/docs/${slug.join("/")}/`;
  return {
    title: `${meta.title} · Docs`,
    description: meta.description,
    alternates: { canonical: canonicalPath },
    openGraph: {
      title: meta.title,
      description: meta.description,
      type: "article",
      url: canonicalPath,
    },
  };
}

export default async function DocPage({ params }: Props) {
  const { slug } = await params;
  const raw = readDoc(slug);
  if (!raw) notFound();
  const md = rewriteDocLinks(raw, slug);
  const githubPath = `${GITHUB_REPO}/blob/main/docs/${slug.join("/")}.md`;
  const isGuide = slug[0] === "guides";
  const currentPath = `/docs/${slug.join("/")}/`;

  return (
    <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6">
      <div className="grid gap-10 lg:grid-cols-[240px_minmax(0,1fr)]">
        <aside className="hidden lg:block">
          <DocsSidebar currentPath={currentPath} />
        </aside>
        <div className="min-w-0">
          <details className="mb-6 rounded-lg border border-[var(--hairline)] bg-[var(--canvas-soft)] p-3 lg:hidden">
            <summary className="cursor-pointer text-sm font-medium text-[var(--ink)]">
              Docs navigation
            </summary>
            <div className="mt-4">
              <DocsSidebar currentPath={currentPath} />
            </div>
          </details>
          <p className="mb-6 text-sm text-[var(--mute)]">
            {isGuide ? (
              <>
                <Link href="/docs/guides/" className="underline">
                  Guides
                </Link>
                {slug.length > 1 ? (
                  <>
                    {" / "}
                    {slug.slice(1).join(" / ")}
                  </>
                ) : null}
              </>
            ) : (
              <>
                <Link href="/docs/guides/" className="underline">
                  Guides
                </Link>
                {" / "}
                {slug.join(" / ")}
              </>
            )}
            {" · "}
            <a href={githubPath} className="underline" target="_blank" rel="noreferrer">
              Edit on GitHub
            </a>
          </p>
          <article className="prose-docs space-y-4 text-[var(--body)] [&_a]:text-[var(--primary)] [&_a]:underline [&_code]:rounded [&_code]:border [&_code]:border-[var(--hairline)] [&_code]:bg-[var(--canvas-soft)] [&_code]:px-1 [&_h1]:font-[family-name:var(--font-serif)] [&_h1]:text-3xl [&_h1]:font-semibold [&_h1]:tracking-tight [&_h1]:text-[var(--ink)] [&_h2]:mt-8 [&_h2]:font-[family-name:var(--font-serif)] [&_h2]:text-xl [&_h2]:font-semibold [&_h2]:text-[var(--ink)] [&_h3]:mt-6 [&_h3]:text-lg [&_h3]:text-[var(--ink)] [&_li]:my-1 [&_p]:leading-relaxed [&_pre]:overflow-x-auto [&_pre]:rounded-lg [&_pre]:border [&_pre]:border-[var(--hairline)] [&_pre]:bg-[var(--canvas-soft)] [&_pre]:p-3 [&_pre]:text-sm [&_table]:w-full [&_table]:text-sm [&_td]:border [&_td]:border-[var(--hairline)] [&_td]:p-2 [&_th]:border [&_th]:border-[var(--hairline)] [&_th]:p-2 [&_th]:text-left">
            <DocMarkdown>{md}</DocMarkdown>
          </article>
        </div>
      </div>
    </div>
  );
}
