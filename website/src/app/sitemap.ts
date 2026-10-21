import type { MetadataRoute } from "next";
import { listDocSlugs } from "@/lib/docs";
import { listLanguages } from "@/lib/languages";

export const dynamic = "force-static";

const SITE = "https://shaaf.dev/rgctl";

export default function sitemap(): MetadataRoute.Sitemap {
  const staticPaths = [
    "/",
    "/install/",
    "/agents/",
    "/demo/",
    "/community/",
    "/docs/guides/",
    "/docs/languages/",
  ];

  const docPaths = listDocSlugs()
    .filter((slug) => !(slug.length === 1 && slug[0] === "guides"))
    .map((slug) => `/docs/${slug.join("/")}/`);

  const langPaths = listLanguages().map((l) => `/docs/languages/${l.id}/`);

  const all = [...new Set([...staticPaths, ...docPaths, ...langPaths])];
  const now = new Date();
  return all.map((path) => ({
    url: `${SITE}${path}`,
    lastModified: now,
    changeFrequency: path === "/" ? "weekly" : "monthly",
    priority: path === "/" ? 1 : path.startsWith("/docs/guides/") ? 0.8 : 0.6,
  }));
}
