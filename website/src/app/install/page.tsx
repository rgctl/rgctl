import type { Metadata } from "next";
import Link from "next/link";
import { TerminalBlock } from "@/components/terminal";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { GITHUB_RELEASES, GITHUB_REPO } from "@/lib/utils";

export const metadata: Metadata = {
  title: "Install",
};

export default function InstallPage() {
  return (
    <div className="mx-auto max-w-6xl px-4 py-14 sm:px-6">
      <Badge className="mb-4">Get started</Badge>
      <h1 className="font-[family-name:var(--font-serif)] text-3xl font-semibold tracking-tight text-[var(--ink)] sm:text-4xl">
        Install rgctl
      </h1>
      <p className="mt-3 text-[var(--body)]">
        Prefer a release binary for day-to-day use. Build from source when you
        need the latest main. Pull Git LFS only if you use the optional
        code-daemon semantic embedder.
      </p>

      <section className="mt-10 space-y-3">
        <h2 className="text-lg text-[var(--ink)]">Option A — One-line installer</h2>
        <p className="text-sm text-[var(--body)]">
          macOS / Linux: download the latest release binary into{" "}
          <code className="font-mono">$HOME/.local/bin</code> (override with{" "}
          <code className="font-mono">RGCTL_INSTALL_DIR</code>):
        </p>
        <TerminalBlock
          lines={[
            "curl -fsSL https://shaaf.dev/rgctl/install.sh | sh",
            "rgctl --version",
            "rgctl install --skill --tools cursor,claude,codex,antigravity,agents",
          ]}
        />
        <p className="text-sm text-[var(--body)]">
          Or download a tarball manually from{" "}
          <a
            href={GITHUB_RELEASES}
            className="text-[var(--primary)] underline"
            target="_blank"
            rel="noreferrer"
          >
            Releases
          </a>
          , put <code className="font-mono text-[var(--body-strong)]">rgctl</code>{" "}
          on your <code className="font-mono">PATH</code>, then install the{" "}
          <strong className="font-medium text-[var(--ink)]">agent pack</strong>{" "}
          (single skill{" "}
          <code className="font-mono text-[var(--body-strong)]">rgctl</code>
          ).
        </p>
        <p className="text-sm text-[var(--mute)]">
          Installs <code className="font-mono">rgctl</code> under each adapter
          (e.g. <code className="font-mono">.cursor/skills/rgctl/</code>). See
          the{" "}
          <Link href="/docs/guides/agent-skill/" className="underline">
            agent pack guide
          </Link>
          .
        </p>
        <Button variant="ghost" asChild>
          <a href={GITHUB_RELEASES} target="_blank" rel="noreferrer">
            Open releases
          </a>
        </Button>
      </section>

      <section className="mt-12 space-y-3">
        <h2 className="text-lg text-[var(--ink)]">Option B — Build from source</h2>
        <TerminalBlock
          lines={[
            "git clone https://github.com/sshaaf/rgctl.git",
            "cd rgctl",
            "# Optional: only if you use `semantic index --embedder code-daemon` (~206 MB)",
            "git lfs pull",
            "cargo build --release --bin rgctl",
            "./target/release/rgctl --version",
          ]}
        />
      </section>

      <section className="mt-12 space-y-3">
        <h2 className="text-lg text-[var(--ink)]">First hour</h2>
        <p className="text-sm text-[var(--body)]">
          Use the in-tree{" "}
          <code className="font-mono text-[var(--body-strong)]">
            rgctl-tests/ecommerce-java
          </code>{" "}
          fixture (canonical walkthrough in the User Guide).
        </p>
        <TerminalBlock
          lines={[
            "cd rgctl-tests/ecommerce-java",
            "rgctl discover .",
            "rgctl -f json find --type function --count-only | jq '.total'",
            'rgctl -f json blast-radius "priceShoppingCart" --depth 2',
          ]}
        />
        <p className="text-sm text-[var(--mute)]">
          Dashboard and migration JSON are opt-in: add{" "}
          <code className="font-mono">--with-dashboard</code> /{" "}
          <code className="font-mono">--export-migration-hints</code>.
        </p>
      </section>

      <section className="mt-12 flex flex-wrap gap-3">
        <Button asChild>
          <Link href="/docs/guides/">Read the guides</Link>
        </Button>
        <Button variant="ghost" asChild>
          <Link href="/agents/">Agents</Link>
        </Button>
        <Button variant="ghost" asChild>
          <Link href="/demo/">Try demos</Link>
        </Button>
        <Button variant="ghost" asChild>
          <a href={`${GITHUB_REPO}/blob/main/docs/user-guide.md`} target="_blank" rel="noreferrer">
            User Guide on GitHub
          </a>
        </Button>
      </section>
    </div>
  );
}
