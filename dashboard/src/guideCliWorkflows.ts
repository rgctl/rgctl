import type { TabId } from "./tabDocs";

export interface CliCommandBlock {
  comment?: string;
  commands: string[];
}

export interface CliWorkflowSection {
  /** Dashboard tab this mirrors (guide uses multiple). */
  tabId: TabId;
  tabLabel: string;
  summary: string;
  prerequisite?: string;
  blocks: CliCommandBlock[];
  notes?: string[];
}

export const GUIDE_PREREQUISITES = `export REPO="$PWD"   # repository root after discover

# Fast index (graph, blast scores, metrics, dashboard)
rbuilder discover .

# Add CFG / PDG / taint + full dashboard analysis (slower)
rbuilder discover . --all`;

export const CLI_WORKFLOWS: CliWorkflowSection[] = [
  {
    tabId: "graph",
    tabLabel: "Graph visualization",
    summary:
      "Explore package/community structure, drill into call neighborhoods, and export subgraphs for external tools.",
    prerequisite: "rbuilder discover .",
    blocks: [
      {
        comment: "List functions and orient by name",
        commands: [
          'rbuilder -r "$REPO" gql --macro-name all_functions unused',
          'rbuilder -r "$REPO" gql "MATCH (n:Function) WHERE n.name LIKE \'*Service*\' RETURN n LIMIT 20"',
        ],
      },
      {
        comment: "Call chains (1–3 hops) — same edges the metagraph summarizes",
        commands: [
          'rbuilder -r "$REPO" gql "MATCH (a:Function)-[:CALLS*1..3]->(b:Function) RETURN a,b LIMIT 50"',
          'rbuilder -r "$REPO" gql --macro-name call_chain unused',
        ],
      },
      {
        comment: "Export a subgraph (GraphML, DOT, Mermaid) for offline layout",
        commands: [
          'rbuilder -r "$REPO" export --export-format graphml --export-output subgraph.graphml \\',
          '  --query "name:ShoppingCartService"',
          'rbuilder -r "$REPO" export --export-format mermaid --export-output calls.mmd --query all',
        ],
      },
      {
        comment: "Many queries in one session — warm graph cache",
        commands: ['rbuilder -r "$REPO" serve', 'rbuilder -r "$REPO" -f json gql "MATCH (n:Function) RETURN n LIMIT 5"'],
      },
    ],
    notes: [
      "Dashboard metagraph drill-down has no single CLI command; combine GQL + export, or use `serve` for repeated queries.",
      "`export --query` uses filter syntax (`name:`, `type:`, `functions`, `all`) — not full GQL MATCH.",
      "Add `-f json` on any command for scripts and CI.",
    ],
  },
  {
    tabId: "functions",
    tabLabel: "Function inventory",
    summary:
      "Browse symbols with PageRank, betweenness, harmonic centrality, and blast scores — same columns as the Functions table.",
    prerequisite: "rbuilder discover .",
    blocks: [
      {
        comment: "Inventory all functions",
        commands: [
          'rbuilder -r "$REPO" gql --macro-name all_functions unused',
          'rbuilder -r "$REPO" -f json gql "MATCH (n:Function) RETURN n" | jq ".rows | length"',
        ],
      },
      {
        comment: "Centrality reports (sort mentally like PR / BC columns)",
        commands: [
          'rbuilder -r "$REPO" metrics --pagerank',
          'rbuilder -r "$REPO" metrics --betweenness',
          'rbuilder -r "$REPO" -f json metrics --pagerank | jq ".rows[:10]"',
        ],
      },
      {
        comment: "Filter by type or file path",
        commands: [
          'rbuilder -r "$REPO" gql "MATCH (n:Class) RETURN n LIMIT 30"',
          'rbuilder -r "$REPO" gql "MATCH (n:Function) WHERE n.file_path LIKE \'*service*\' RETURN n"',
        ],
      },
    ],
    notes: [
      "Blast scores in the table come from discover; re-run `blast-radius <symbol>` for caller lists at a chosen depth.",
    ],
  },
  {
    tabId: "cfg",
    tabLabel: "CFG / PDG analysis",
    summary:
      "Inspect control-flow blocks, branches, and dominance inside one function — equivalent to the CFG graph + dominance panel.",
    prerequisite: "rbuilder discover . --cfg   # or discover . --all",
    blocks: [
      {
        comment: "CFG summary and diagram",
        commands: [
          'rbuilder -r "$REPO" inspect ShoppingCartService cfg',
          'rbuilder -r "$REPO" -f mermaid inspect ShoppingCartService cfg',
          'rbuilder -r "$REPO" inspect ShoppingCartService cfg --prune',
        ],
      },
      {
        comment: "Dominator tree + frontiers (dominance panel)",
        commands: ['rbuilder -r "$REPO" inspect ShoppingCartService dom --frontiers'],
      },
      {
        comment: "Disambiguate common symbol names (use a unique function name or FQN)",
        commands: [
          'rbuilder -r "$REPO" inspect ShoppingCartService.process cfg',
          'rbuilder -r "$REPO" blast-radius process --class ShoppingCartService',
        ],
      },
    ],
    notes: [
      "`inspect` takes a function symbol only (no `--class`); use a unique name or `blast-radius` when names collide.",
      "Large repos: dashboard loads one function on demand from the CFG archive; CLI `inspect` reads the same `.rbuilder/analysis/` cache.",
    ],
  },
  {
    tabId: "dataflow",
    tabLabel: "Dataflow",
    summary:
      "Statement-level data and control dependencies inside a function — Data Flow and Dominator Tree views.",
    prerequisite: "rbuilder discover . --cfg   # or discover . --all",
    blocks: [
      {
        comment: "PDG / dataflow edges",
        commands: [
          'rbuilder -r "$REPO" inspect ShoppingCartService pdg --edge-layer data',
          'rbuilder -r "$REPO" inspect ShoppingCartService pdg --def-use',
          'rbuilder -r "$REPO" -f mermaid inspect ShoppingCartService pdg --edge-layer data',
        ],
      },
      {
        comment: "Dominator tree (same as Dataflow → Dominator Tree view)",
        commands: ['rbuilder -r "$REPO" inspect ShoppingCartService dom --frontiers'],
      },
      {
        comment: "Slice with CFG/PDG overlay on one variable",
        commands: [
          'rbuilder -r "$REPO" -f mermaid slice src/main/java/.../ShoppingCartService.java \\',
          "  --line 45 --variable cart --function ShoppingCartService --view pdg",
        ],
      },
    ],
  },
  {
    tabId: "slice",
    tabLabel: "Program slicing",
    summary:
      "Backward or forward line-level slice for a variable at a line — same as Compute slice in the dashboard.",
    prerequisite: "rbuilder discover . --cfg recommended for cross-function context",
    blocks: [
      {
        comment: "Backward slice — what influences this point?",
        commands: [
          'rbuilder -r "$REPO" slice src/main/java/.../ShoppingCartService.java \\',
          "  --line 45 --variable cart --function ShoppingCartService",
        ],
      },
      {
        comment: "Forward slice — what does this point affect?",
        commands: [
          'rbuilder -r "$REPO" slice src/main/java/.../CartEndpoint.java \\',
          "  --line 37 --variable cartId --function CartEndpoint --direction forward",
        ],
      },
      {
        comment: "JSON for automation",
        commands: [
          'rbuilder -r "$REPO" -f json slice src/main/java/.../ShoppingCartService.java \\',
          "  --line 45 --variable cart --function ShoppingCartService | jq .lines",
        ],
      },
    ],
  },
  {
    tabId: "blast",
    tabLabel: "Blast radius",
    summary:
      "Upstream impact if you change a symbol — impact score, direct callers, and impact zone (dashboard table is depth-limited).",
    prerequisite: "rbuilder discover .",
    blocks: [
      {
        comment: "Full-graph blast (matches sidebar summary cards)",
        commands: [
          'rbuilder -r "$REPO" blast-radius ShoppingCartService',
          'rbuilder -r "$REPO" -f json blast-radius ShoppingCartService | jq ".metrics"',
        ],
      },
      {
        comment: "Limit caller depth (matches dashboard depth slider)",
        commands: [
          'rbuilder -r "$REPO" blast-radius ShoppingCartService --depth 1',
          'rbuilder -r "$REPO" blast-radius ShoppingCartService --depth 5',
          'rbuilder -r "$REPO" -f json blast-radius ShoppingCartService --depth 5 | jq ".topology.callers[:10]"',
        ],
      },
      {
        comment: "Disambiguate symbols",
        commands: [
          'rbuilder -r "$REPO" blast-radius process --class ShoppingCartService',
          'rbuilder -r "$REPO" blast-radius getCLogger --file src/main/java/.../CLogger.java',
        ],
      },
      {
        comment: "CI policy gate on changed functions",
        commands: ['rbuilder -r "$REPO" check --policy-file policy.json'],
      },
    ],
    notes: [
      "Sidebar scores are full-graph metrics from discover; the caller table respects the depth slider.",
    ],
  },
  {
    tabId: "taint",
    tabLabel: "Taint analysis",
    summary:
      "Source-to-sink flows and sanitizer checks per function — requires CFG/PDG from discover.",
    prerequisite: "rbuilder discover . --cfg   # or discover . --all",
    blocks: [
      {
        comment: "Taint trace at a program point",
        commands: [
          'rbuilder -r "$REPO" slice src/main/java/.../ShoppingCartService.java \\',
          "  --line 48 --variable cart --function ShoppingCartService --taint",
          'rbuilder -r "$REPO" -f json slice src/main/java/.../ShoppingCartService.java \\',
          "  --line 48 --variable cart --function ShoppingCartService --taint | jq .",
        ],
      },
      {
        comment: "Find candidate entrypoints, then trace",
        commands: [
          'rbuilder -r "$REPO" gql "MATCH (n:Function) WHERE n.name LIKE \'*Endpoint\' RETURN n"',
          'rbuilder -r "$REPO" slice <file> --line <N> --variable <VAR> --function <Class> --taint',
        ],
      },
    ],
    notes: [
      "Dashboard taint tab lists flows exported at discover time; CLI `slice --taint` re-runs analysis on demand.",
    ],
  },
  {
    tabId: "migration",
    tabLabel: "Migration planner",
    summary:
      "Package-level extraction roadmap from communities, centrality, and blast — export plan JSON for agents or CI.",
    prerequisite: "rbuilder discover . --all --export-migration-plan",
    blocks: [
      {
        comment: "Default hybrid strategy",
        commands: [
          'rbuilder discover . --all --export-migration-plan',
          'cat .rbuilder/dashboard/migration_plan.json | jq ".packages[:5]"',
        ],
      },
      {
        comment: "Strategy presets (dashboard α/β/γ presets)",
        commands: [
          'rbuilder discover . --all --export-migration-plan --migration-preset risk_mitigation',
          'rbuilder discover . --all --export-migration-plan --migration-preset hotspot_first',
          'rbuilder discover . --all --export-migration-plan --migration-order priority',
        ],
      },
      {
        comment: "Machine-readable discover + plan",
        commands: [
          'rbuilder -f json discover . --all --export-migration-plan | jq ".migration_plan_path"',
        ],
      },
    ],
    notes: [
      "Interactive weight tuning is dashboard-only; re-run discover with presets to refresh CLI exports.",
      "Graph layout: `.rbuilder/dashboard/migration_graph.json` after export.",
    ],
  },
  {
    tabId: "guide",
    tabLabel: "GQL reference",
    summary: "Core graph queries used across tabs — patterns, macros, and JSON output.",
    prerequisite: "rbuilder discover .",
    blocks: [
      {
        comment: "Macros (shortcuts)",
        commands: [
          'rbuilder -r "$REPO" gql --macro-name all_functions unused',
          'rbuilder -r "$REPO" gql --macro-name direct_calls unused',
          'rbuilder -r "$REPO" gql --macro-name call_chain unused',
        ],
      },
      {
        comment: "Patterns",
        commands: [
          'rbuilder -r "$REPO" -f json gql "MATCH (n:Function) RETURN n" | jq ".count"',
          'rbuilder -r "$REPO" gql "MATCH (a:Function)-[:CALLS]->(b:Function) RETURN a,b LIMIT 25"',
          'rbuilder -r "$REPO" gql --explain "MATCH (n:Function) WHERE n.name = \'Foo\' RETURN n"',
        ],
      },
      {
        comment: "Automation",
        commands: [
          'rbuilder -r "$REPO" -f json gql "MATCH (n:Function) RETURN n" | jq ".rows[0]"',
          'rbuilder -f json discover . | jq ".metrics"',
        ],
      },
    ],
  },
];
