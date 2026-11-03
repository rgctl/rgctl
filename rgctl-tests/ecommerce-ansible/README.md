Static ansible ecommerce graph fixture. Inventory/config companions remain generic configuration.
Macros, blocks and synthetic render scopes are Function nodes; only exact macro invocations are Calls.
Template/role/task reuse is a typed dependency, so call-only blast radius does not traverse it.
The CLI integration suites assert exact role/template endpoints, forbidden cross-role edges and persisted unresolved evidence.
No rendering, inventory execution, precedence, CFG/PDG or taint support is claimed.
