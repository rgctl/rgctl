import type { MetadataRoute } from "next";

export const dynamic = "force-static";

export default function robots(): MetadataRoute.Robots {
  return {
    rules: {
      userAgent: "*",
      allow: "/",
    },
    sitemap: "https://shaaf.dev/rgctl/sitemap.xml",
    host: "https://shaaf.dev/rgctl",
  };
}
