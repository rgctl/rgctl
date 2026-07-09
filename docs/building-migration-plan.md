## The rBuilder-Driven Migration Plan

### Phase 1: Landscape Inventory & Mapping

**Goal:** Map out the existing repository to understand its boundaries and size before touching any code.

* **Step 1:** Run `rbuilder discover . --all` to index the entire repository. This creates the foundational map under `.rbuilder/`.
* **Step 2:** Use **Graph Queries (GQL)** with macros like `all_functions` and `call_chain` to inventory modules, list cross-boundary dependencies, and get an accurate baseline of the current system architecture.
* **Step 3:** Generate a visual landscape using the **Dashboard** (`cd .rbuilder/dashboard && python3 -m http.server`) to identify tight structural coupling and package boundaries interactively.

### Phase 2: Hotspot & Risk Assessment

**Goal:** Identify which parts of the codebase are safe to migrate first and which are highly dangerous "hotspots."

* **Step 1:** Run **Graph Metrics** to calculate **PageRank** and **Betweenness** centrality.
* *Low PageRank/Betweenness:* Good targets for early, low-risk migration phases.
* *High PageRank/Betweenness:* "Bridge" or core utility nodes that require maximum caution.


* **Step 2:** Identify **Communities** (densely connected clusters) using the metrics tool to carve out logical microservices or migration batches.

### Phase 3: Impact ("Blast Radius") Analysis

**Goal:** For any specific component selected for migration, determine exactly what will break upstream.

* **Step 1:** Run `rbuilder blast-radius` on targeted functions or modules slated for changes.
* **Step 2:** Evaluate the impact **score** and the **impact zone** list. If a function has a massive upstream impact zone, use the `--depth` flag to isolate immediate callers and plan incremental wrapper/adapter layers.
* **Step 3:** Pipe the output via `-f json` to save a stable record of canonical names/UUIDs that must be tested post-migration.

### Phase 4: Execution & Precision Extraction

**Goal:** Safely refactor, untangle, or extract the selected code.

* **Step 1:** Use **Program Slicing** (`slice`) to isolate the exact data and control dependencies of variables within highly complex functions. This ensures you only move the lines of code that actually matter to that feature.
* **Step 2:** Run **Taint Analysis** (`slice --taint`) on migrated code blocks to verify that moving components doesn't inadvertently introduce security vulnerabilities (e.g., exposing an unsanitized input sink in the new environment).
* **Step 3:** Use **Export** (`rbuilder export --format mermaid` or `dot`) to extract subgraphs of the migrated features for architectural documentation and peer reviews.

### Phase 5: Governance & CI Guardrails

**Goal:** Ensure that ongoing work does not violate the new migration boundaries or re-introduce legacy dependencies.

* **Step 1:** Write an rBuilder **Policy File** outlining forbidden cross-domain impacts or preventing calls back to legacy modules.
* **Step 2:** Integrate `rbuilder check` into your **CI policy checks** (Pull Request pipeline). If a developer introduces a change that violates the migration architecture boundaries, the CI pipeline will return exit code `1` and block the merge.