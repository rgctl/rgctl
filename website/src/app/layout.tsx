import type { Metadata } from "next";
import Script from "next/script";
import { IBM_Plex_Mono, Inter, Source_Serif_4 } from "next/font/google";
import { Analytics } from "@/components/analytics";
import { SiteFooter } from "@/components/site-footer";
import { SiteHeader } from "@/components/site-header";
import {
  ThemeProvider,
  themeInitScript,
} from "@/components/theme-provider";
import "./globals.css";

const inter = Inter({
  subsets: ["latin"],
  variable: "--font-sans",
  display: "swap",
});

const sourceSerif = Source_Serif_4({
  subsets: ["latin"],
  variable: "--font-serif",
  display: "swap",
});

const ibmPlexMono = IBM_Plex_Mono({
  weight: ["400", "500"],
  subsets: ["latin"],
  variable: "--font-mono",
  display: "swap",
});

export const metadata: Metadata = {
  title: {
    default: "rgctl — code knowledge graph for AI agents",
    template: "%s · rgctl",
  },
  description:
    "Open-source code knowledge graph for LLM agents. Index once, query compact JSON — blast radius, structured find/callers, semantic search, migration planning.",
  metadataBase: new URL("https://shaaf.dev/rgctl"),
  openGraph: {
    title: "rgctl — code knowledge graph for AI agents",
    description:
      "Open-source code knowledge graph for LLM agents — accurate answers, fewer tokens.",
    type: "website",
    url: "https://shaaf.dev/rgctl/",
    siteName: "rgctl",
  },
  twitter: {
    card: "summary_large_image",
    title: "rgctl — code knowledge graph for AI agents",
    description:
      "Open-source code knowledge graph for LLM agents — accurate answers, fewer tokens.",
  },
};

const softwareApplicationLd = {
  "@context": "https://schema.org",
  "@type": "SoftwareApplication",
  name: "rgctl",
  applicationCategory: "DeveloperApplication",
  operatingSystem: "macOS, Linux, Windows",
  license: "https://opensource.org/licenses/MIT",
  codeRepository: "https://github.com/sshaaf/rgctl",
  url: "https://shaaf.dev/rgctl/",
  description:
    "Open-source code knowledge graph for LLM agents. Index once, query compact JSON — blast radius, structured find/callers, semantic search, migration planning.",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html
      lang="en"
      className={`${inter.variable} ${sourceSerif.variable} ${ibmPlexMono.variable} h-full`}
      suppressHydrationWarning
    >
      <body className="flex min-h-full flex-col antialiased">
        <Script id="theme-init" strategy="beforeInteractive">
          {themeInitScript}
        </Script>
        <Script
          id="software-application-ld"
          type="application/ld+json"
          dangerouslySetInnerHTML={{
            __html: JSON.stringify(softwareApplicationLd),
          }}
        />
        <ThemeProvider>
          <SiteHeader />
          <main className="flex-1">{children}</main>
          <SiteFooter />
          <Analytics />
        </ThemeProvider>
      </body>
    </html>
  );
}
