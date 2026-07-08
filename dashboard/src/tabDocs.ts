export type TabId =
  | "graph"
  | "functions"
  | "cfg"
  | "dataflow"
  | "taint"
  | "guide"
  | "slice"
  | "blast";

export interface TabDocContent {
  title: string;
  goal: string;
  description: string;
  benefits: string[];
  usage: string[];
}

export const TAB_DOCS: Record<TabId, TabDocContent> = {
  graph: {
    title: "Graph visualization",
    goal: "Explore the codebase structure interactively — package overview, drill-down, and neighborhood inspection.",
    description:
      "This view shows a metagraph of packages and communities from your indexed repository. Click a package or community to drill into functions and calls. The same graph snapshot powers CLI queries; here you navigate visually without memorizing syntax.",
    benefits: [
      "Visual navigation for large monorepos",
      "Quick orientation for onboarding and demos",
      "Drill from overview to function-level subgraphs",
    ],
    usage: [
      "Search for a function or package name in the toolbar.",
      "Click a metanode to expand its internal graph.",
      "Use breadcrumbs to zoom back out.",
      "Toggle edge types and fit the view from the toolbar controls.",
    ],
  },
  functions: {
    title: "Function inventory",
    goal: "Browse and filter indexed nodes — functions, classes, and other symbols — with complexity and blast scores.",
    description:
      "Like a structural inventory of the graph: see names, types, cyclomatic complexity, and precomputed blast-radius scores. Use this to find hotspots before diving into CFG, blast radius, or slicing views.",
    benefits: [
      "Fast orientation in unfamiliar repos",
      "Filter by node type to focus on functions or classes",
      "Spot high-complexity or high-impact symbols at a glance",
    ],
    usage: [
      "Choose a node-type filter at the top.",
      "Page through results with Prev / Next (30 per page).",
      "Use blast and complexity columns to prioritize reading order.",
    ],
  },
  cfg: {
    title: "CFG / PDG analysis",
    goal: "Inspect how code executes inside a single function — branches, loops, and control flow.",
    description:
      "The control-flow graph (CFG) shows blocks and branches: what can run after what. This view requires `discover --cfg` or `discover --all` so CFG data is exported into the dashboard bundle.",
    benefits: [
      "Compiler-minded debugging without leaving the repo toolchain",
      "Foundation for slice, taint, and dataflow features",
      "Block-level view of branching and loops",
    ],
    usage: [
      "Select a function from the list (search and paginate as needed).",
      "Read the CFG graph — edges are colored by branch type.",
      "Review the entry-block preview and block table below the graph.",
    ],
  },
  dataflow: {
    title: "Dataflow",
    goal: "See data and control dependencies between statements inside a function.",
    description:
      "The program dependence graph (PDG) connects statements through data and control edges. Filter by variable to highlight def-use paths relevant to a specific name. Requires CFG/PDG export from discover.",
    benefits: [
      "Trace how values flow between statements",
      "Narrow large graphs with a variable filter",
      "Complement slicing with a visual dependency map",
    ],
    usage: [
      "Pick a function from the sidebar list.",
      "Choose Data Flow (CFG + PDG) or Dominator Tree from the view dropdown.",
      "Optionally filter by variable; toggle control deps and CFG edges.",
    ],
  },
  taint: {
    title: "Taint analysis",
    goal: "Find flows where untrusted input (sources) may reach dangerous operations (sinks).",
    description:
      "Taint analysis tracks data from sources such as request parameters or files to sinks such as SQL, shell, or HTML output. Flows may be sanitized on the path; vulnerable flows lack an effective sanitizer. Produced when you run `discover --cfg` or `--all`.",
    benefits: [
      "Security review without manual path tracing on every endpoint",
      "Per-function flow lists with severity context",
      "Batch reporting across the indexed codebase",
    ],
    usage: [
      "Select a function from the list.",
      "Review source-to-sink flows in the detail panel.",
      "Functions with vulnerabilities show a badge in the sidebar.",
    ],
  },
  guide: {
    title: "Query guide (GQL)",
    goal: "Explore and inventory the codebase using graph patterns — like SQL for structure.",
    description:
      "GQL matches patterns in the indexed graph: find functions by name, list call chains, or count nodes by type. Named macros bundle common patterns. The CLI is the primary interface; examples below work after `discover`.",
    benefits: [
      "Repeatable audits on every release",
      "Automation with `-f json` and scripts",
      "Deterministic answers from the indexed graph",
    ],
    usage: [
      "Run `rbuilder discover .` once per repo (or after large changes).",
      "Use `rbuilder gql` with MATCH patterns from the examples below.",
      "Add `-f json` for machine-readable output in CI or agents.",
    ],
  },
  slice: {
    title: "Program slicing",
    goal: "Find which lines in a function actually affect a variable at a chosen line.",
    description:
      "Slicing computes the minimal set of statements that influence (backward) or are influenced by (forward) a program point. Point at a line and variable; rBuilder highlights the slice using control-flow and dependence structure inside the function.",
    benefits: [
      "Narrow focus during incident response",
      "Less noise than reading the entire file",
      "Backward and forward slice directions",
    ],
    usage: [
      "Select a function from the sidebar.",
      "Set line number, variable name, and slice direction.",
      "Run Compute slice and review highlighted source lines.",
    ],
  },
  blast: {
    title: "Blast radius",
    goal: "See what breaks upstream if you change a function — before you merge.",
    description:
      "Blast radius walks the incoming call graph from a chosen symbol. You get an impact score, direct callers, and the wider impact zone up to a caller depth. Functions are sorted by impact score so high-risk symbols are easy to find.",
    benefits: [
      "Change-risk triage before code review or release",
      "Refactoring safety before renaming or deleting APIs",
      "Depth control for how far upstream to search",
    ],
    usage: [
      "Pick a high-impact function from the sorted list (search and paginate as needed).",
      "Adjust caller depth to widen or narrow the impact zone.",
      "Review the caller table and impact metrics in the detail panel.",
    ],
  },
};

export const TAB_DOC_IDS = Object.keys(TAB_DOCS) as TabId[];
