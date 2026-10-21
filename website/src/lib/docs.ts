import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const contentRoot = join(process.cwd(), "content/docs");

export function docsRoot(): string {
  return contentRoot;
}

export function listDocSlugs(): string[][] {
  if (!existsSync(contentRoot)) return [];
  const out: string[][] = [];
  function walk(dir: string, parts: string[]) {
    for (const name of readdirSync(dir)) {
      const fp = join(dir, name);
      if (statSync(fp).isDirectory()) {
        walk(fp, [...parts, name]);
        continue;
      }
      if (!name.endsWith(".md")) continue;
      const stem = name.replace(/\.md$/, "");
      out.push([...parts, stem]);
      // Also emit the directory-level slug so /docs/DIR/ resolves to README.md
      if (stem === "README" && parts.length > 0) {
        out.push(parts);
      }
    }
  }
  walk(contentRoot, []);
  return out;
}

export function readDoc(slug: string[]): string | null {
  const candidates = [
    join(contentRoot, ...slug) + ".md",
    join(contentRoot, ...slug, "README.md"),
  ];
  for (const c of candidates) {
    if (existsSync(c)) return readFileSync(c, "utf8");
  }
  return null;
}

/**
 * Determine the directory context for a slug (used for resolving relative links).
 * If the slug resolves to a README.md, the slug itself is the directory.
 * Otherwise, the directory is the slug minus its last segment.
 */
export function slugDir(slug: string[]): string[] {
  // Check if the slug resolves via the README.md fallback
  const direct = join(contentRoot, ...slug) + ".md";
  if (existsSync(direct)) {
    // Resolved as <slug>.md — directory is everything except the last segment
    return slug.length > 1 ? slug.slice(0, -1) : [];
  }
  // Resolved as <slug>/README.md — the slug itself is the directory
  return slug;
}

export type DocMeta = {
  title: string;
  description: string;
};

/** Strip light markdown formatting for meta titles/descriptions. */
function stripMdInline(s: string): string {
  return s
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/`([^`]+)`/g, "$1")
    .replace(/\*\*([^*]+)\*\*/g, "$1")
    .replace(/\*([^*]+)\*/g, "$1")
    .replace(/_([^_]+)_/g, "$1")
    .trim();
}

function titleCaseSlug(slug: string): string {
  return slug
    .split(/[-_]/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");
}

/**
 * Extract `# H1` title and lead paragraph for SEO metadata.
 * Falls back to a title-cased slug segment when no H1 is present.
 */
export function extractDocMeta(md: string, fallbackSlug: string): DocMeta {
  const lines = md.split(/\r?\n/);
  let title: string | null = null;
  let description = "";
  let pastTitle = false;
  for (const line of lines) {
    const h1 = line.match(/^#\s+(.+)$/);
    if (!pastTitle && h1) {
      title = stripMdInline(h1[1]);
      pastTitle = true;
      continue;
    }
    if (!pastTitle) continue;
    const trimmed = line.trim();
    if (!trimmed) {
      if (description) break;
      continue;
    }
    if (/^#{1,6}\s/.test(trimmed)) break;
    if (/^[-*_|`]/.test(trimmed) && !/^[A-Za-z]/.test(trimmed)) {
      // skip table/list/code fences as lead
      if (trimmed.startsWith("```") || trimmed.startsWith("|")) continue;
    }
    if (/^```/.test(trimmed)) continue;
    description = stripMdInline(trimmed);
    break;
  }
  if (!title) {
    title = titleCaseSlug(fallbackSlug || "docs");
  }
  if (description.length > 155) {
    const cut = description.slice(0, 155);
    const sp = cut.lastIndexOf(" ");
    description = (sp > 80 ? cut.slice(0, sp) : cut).trimEnd() + "…";
  }
  if (!description) {
    description = `${title} — rgctl documentation.`;
  }
  return { title, description };
}

/** Rewrite repo-relative markdown links for the site `/docs/` tree. */
export function rewriteDocLinks(md: string, slug?: string[]): string {
  const base = process.env.NEXT_PUBLIC_BASE_PATH || "";
  const prefix = `${base}/docs`;

  // Determine the directory context from the slug so relative links resolve
  // correctly for documents in subdirectories (e.g. guides/README → guides/).
  const dirParts = slug ? slugDir(slug) : [];
  const dirPrefix = dirParts.length > 0 ? dirParts.join("/") + "/" : "";

  return md.replace(/\]\(([^)]+)\)/g, (full, target: string) => {
    if (/^(https?:|mailto:|#|\/)/i.test(target)) return full;
    const hashIdx = target.indexOf("#");
    const pathPart = hashIdx >= 0 ? target.slice(0, hashIdx) : target;
    const hash = hashIdx >= 0 ? target.slice(hashIdx) : "";
    if (!pathPart.endsWith(".md") && pathPart !== "../AGENTS.md") {
      return full;
    }
    let rel = pathPart.replace(/^\.\//, "");
    if (rel === "../AGENTS.md" || rel === "AGENTS.md") {
      return `](${prefix}/AGENTS/${hash})`;
    }

    // Resolve parent-directory references (../) against the current directory
    let resolvedDir = dirPrefix;
    while (rel.startsWith("../")) {
      rel = rel.slice(3);
      // Strip one directory level from resolvedDir
      const parts = resolvedDir.replace(/\/$/, "").split("/").filter(Boolean);
      parts.pop();
      resolvedDir = parts.length > 0 ? parts.join("/") + "/" : "";
    }

    rel = rel.replace(/\.md$/, "");
    return `](${prefix}/${resolvedDir}${rel}/${hash})`;
  });
}
