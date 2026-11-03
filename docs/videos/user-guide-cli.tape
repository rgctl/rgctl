# Comprehensive rgctl CLI showcase (ecommerce-java)
#
# Record: ./docs/videos/record-user-guide-cli.sh
# Burn captions: ./docs/videos/burn-user-guide-captions.sh

Output "docs/videos/user-guide-cli-no-captions.gif"
Output "docs/videos/user-guide-cli-no-captions.mp4"

Require rgctl
Require jq

Set Shell "bash"
Set FontSize 15
Set FontFamily "Menlo, Monaco, Consolas, monospace"
Set Width 1400
Set Height 800
Set Padding 16
Set Margin 20
Set MarginFill "#0d1117"
Set WindowBar Colorful
Set WindowBarSize 40
Set BorderRadius 8
Set Theme "Catppuccin Mocha"
Set TypingSpeed 18ms
Set Framerate 30
Set PlaybackSpeed 1.0
Set CursorBlink true

Hide
Type "cd rgctl-tests/ecommerce-java && rm -rf .rgctl"
Enter
Sleep 300ms
Type "export PS1='\[\e[1;36m\]ecommerce-java\[\e[0m\] \[\e[1;32m\]❯\[\e[0m\] '"
Enter
Sleep 100ms
Type "clear"
Enter
Show

# --- Scene 1: Discover (Graph + CFG/PDG + Kantra + Migration Plan) ---
Type "# 1. Index codebase: AST + mmap graph + CFG/PDG + Kantra + migration plan"
Enter
Type "rgctl discover . -l java -e target --with-cfg --with-kantra --export-migration-hints"
Enter
Sleep 3.5s

Hide
Type "clear"
Enter
Show

# --- Scene 2: Status, Find & Callers ---
Type "# 2. Readiness status, exact symbol lookup & callers (<2ms mmap queries)"
Enter
Type "rgctl -f json status | jq '{status, schema_version, nodes, edges}'"
Enter
Sleep 1.5s
Type "rgctl find clearCart --type function --exact"
Enter
Sleep 1.5s
Type "rgctl callers clearCart --class CartService"
Enter
Sleep 2.5s

Hide
Type "clear"
Enter
Show

# --- Scene 3: Blast Radius (Upstream Change Impact) ---
Type "# 3. Transitive upstream blast radius before you edit a method"
Enter
Type "rgctl blast-radius 'CartService::clearCart'"
Enter
Sleep 2s
Type "rgctl blast-radius 'ShoppingCartService::priceShoppingCart'"
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

# --- Scene 4: Fast-Leiden Communities & PageRank Hotspots ---
Type "# 4. Fast-Leiden domain communities & production-aware PageRank hotspots"
Enter
Type "rgctl -f json communities list | jq '.communities[:4]'"
Enter
Sleep 2s
Type "rgctl -f json metrics --pagerank | jq '.pagerank.top[:3] | map({kind, qualified_name, pagerank})'"
Enter
Sleep 2.5s

Hide
Type "clear"
Enter
Show

# --- Scene 5: Hybrid CPG — CFG, Field Mutations & Backward Program Slicing ---
Type "# 5. Hybrid CPG: control flow, field writes & backward program slicing"
Enter
Type "rgctl -f json inspect checkout --class OrderService cfg | jq '{symbol, layer, blocks: (.nodes | length), edges: (.edges | length)}'"
Enter
Sleep 2s
Type "rgctl cpg mutations --type ShoppingCart --exclude-ctors"
Enter
Sleep 2s
Type "rgctl slice src/main/java/com/example/ecommerce/coolstore/service/ShoppingCartService.java --line 116 --variable result --with-alias"
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

# --- Scene 6: Structural Clone Detection & Semantic Code Search ---
Type "# 6. Structural clone detection & offline semantic search (no GPU needed)"
Enter
Type "rgctl -f json clones --mode bloom --min-loc 3 | jq '{mode, groups: (.groups | length), top_group: .groups[0]}'"
Enter
Sleep 2.5s
Type "rgctl semantic index && rgctl semantic query 'shopping cart promo pricing' --limit 3"
Enter
Sleep 3s

Hide
Type "clear"
Enter
Show

# --- Scene 7: Kantra Migration Rules, Policy Gates, PR Review & Agent Skills ---
Type "# 7. Kantra violations, CI policy gates, PR call-path deltas & AI agent skills"
Enter
Type `rgctl -f json relations --edge violates --from-type kantrarule --limit 3 | jq '.edges | map({rule: .source.name, target: .target.name})'`
Enter
Sleep 2s
Type "rgctl check --policy-file ../rgctl-policy.json"
Enter
Sleep 1.5s
Type "rgctl -f json review paths --base-ref HEAD~1 --head-ref HEAD | jq '{schema_version, changed_functions: (.changed_functions | length), impacted_callers: (.impacted_callers | length)}'"
Enter
Sleep 2s
Type "rgctl -f json install --list-agents | jq '.agents[:5] | map({id, skills_path, invoke_prefix})'"
Enter
Sleep 3s
