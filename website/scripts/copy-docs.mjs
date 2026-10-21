import {
  cpSync,
  existsSync,
  mkdirSync,
  readdirSync,
  statSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, relative } from "node:path";
import { fileURLToPath } from "node:url";

const here = dirname(fileURLToPath(import.meta.url));
const repoRoot = join(here, "../..");
const srcDocs = join(repoRoot, "docs");
const destDocs = join(here, "../content/docs");
const publicDir = join(here, "../public");

function copyTree(from, to) {
  mkdirSync(to, { recursive: true });
  for (const name of readdirSync(from)) {
    if (
      name === "internal" ||
      name === "videos" ||
      name === "images" ||
      name === "languages"
    ) {
      // languages/ is obsolete — site renders from *-ast-coverage.json
      // videos/images handled separately or skipped for v1 text docs
      if (name === "images") {
        const imgFrom = join(from, name);
        const imgTo = join(to, name);
        cpSync(imgFrom, imgTo, { recursive: true });
      }
      continue;
    }
    const fp = join(from, name);
    const tp = join(to, name);
    if (statSync(fp).isDirectory()) {
      copyTree(fp, tp);
    } else if (name.endsWith(".md") || name.endsWith(".txt")) {
      cpSync(fp, tp);
    }
  }
}

if (!existsSync(srcDocs)) {
  console.warn("[copy-docs] missing docs/ — skip");
  process.exit(0);
}

mkdirSync(destDocs, { recursive: true });
// clean slate
cpSync(srcDocs, destDocs, {
  recursive: true,
  filter: (src) => {
    const rel = relative(srcDocs, src);
    if (!rel) return true;
    if (rel.startsWith("internal")) return false;
    if (rel.startsWith("videos")) return false;
    if (rel === "languages" || rel.startsWith("languages/")) return false;
    // keep md/txt/images
    if (statSync(src).isDirectory()) return true;
    return (
      src.endsWith(".md") ||
      src.endsWith(".txt") ||
      /\.(png|jpg|jpeg|gif|svg|webp)$/i.test(src)
    );
  },
});

// Also copy AGENTS.md as agents.md for site
const agents = join(repoRoot, "AGENTS.md");
if (existsSync(agents)) {
  cpSync(agents, join(destDocs, "AGENTS.md"));
}

const installSh = join(repoRoot, "scripts/install.sh");
if (existsSync(installSh)) {
  mkdirSync(publicDir, { recursive: true });
  cpSync(installSh, join(publicDir, "install.sh"));
  console.log("[copy-docs] scripts/install.sh → website/public/install.sh");
}

const llmsTxt = `# rgctl

> Open-source code knowledge graph for LLM agents. Index once (\`discover\`), then query compact JSON — callers, blast radius, CFG/PDG slices, communities, semantic search, OSV reachability, and migration planning.

Website: https://shaaf.dev/rgctl/
Repository: https://github.com/sshaaf/rgctl
Install: \`curl -fsSL https://shaaf.dev/rgctl/install.sh | sh\`

## Capabilities

- Typed code knowledge graph (tree-sitter, 14 Tier-1 languages)
- Structured queries: \`find\`, \`callers\`, \`callees\`, \`relations\`, \`inventory\`
- Blast radius, communities, PageRank / betweenness metrics
- Hybrid CPG / CFG / PDG / program slicing (\`discover --with-cfg\`)
- Semantic search over functions and docs
- OSV / vuln reachability and OpenVEX
- Agent skills via \`rgctl install --skill\`

## CLI (JSON-first)

- \`rgctl discover .\` — index repo → \`.rgctl/\`
- \`rgctl -f json find|callers|blast-radius|cpg|slice|metrics|status\`
- \`rgctl semantic index\` / \`rgctl semantic query\`
- \`rgctl review paths\` / \`rgctl review check\` (PR gates)
- \`rgctl clones\` — exact / bloom / fragment clone detection
- \`rgctl rules\` / \`discover --with-kantra\` — Konveyor-style rules

## Guides

- https://shaaf.dev/rgctl/docs/guides/
- https://shaaf.dev/rgctl/docs/guides/discovering-and-indexing/
- https://shaaf.dev/rgctl/docs/guides/agent-skill/
- https://shaaf.dev/rgctl/docs/guides/blast-radius-analysis/
- https://shaaf.dev/rgctl/docs/guides/hybrid-cpg/
- https://shaaf.dev/rgctl/docs/guides/vulnerability-and-osv-analysis/
- https://shaaf.dev/rgctl/docs/guides/migration-planning/

## Reference

- https://shaaf.dev/rgctl/docs/user-guide/
- https://shaaf.dev/rgctl/docs/json-api/
- https://shaaf.dev/rgctl/docs/policy-format/
- https://shaaf.dev/rgctl/docs/languages/

## Optional

- https://shaaf.dev/rgctl/docs/contributor-checklist/
- https://shaaf.dev/rgctl/docs/analysis-architecture/
- https://shaaf.dev/rgctl/llms.txt
`;

mkdirSync(publicDir, { recursive: true });
writeFileSync(join(publicDir, "llms.txt"), llmsTxt, "utf8");
console.log("[copy-docs] wrote website/public/llms.txt");

console.log(`[copy-docs] docs/ → website/content/docs (excl. internal/videos binaries)`);
