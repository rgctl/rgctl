# User Acceptance Testing - File Manifest

**Created**: June 18, 2026  
**Total Files**: 21  
**Total Size**: ~85 KB

---

## Documentation (4 files)

| File | Size | Purpose |
|------|------|---------|
| `README.md` | 3.2 KB | Quick start guide and overview |
| `UAT_PLAN.md` | 18.5 KB | Detailed test methodology and plan |
| `ACCEPTANCE_CHECKLIST.md` | 9.8 KB | Sign-off document with criteria |
| `TESTING_SUMMARY.md` | 7.3 KB | Executive summary and quick reference |
| `MANIFEST.md` | This file | Complete file listing |

**Purpose**: Help users understand, execute, and sign off on UAT

---

## Test Scripts (7 files)

| Script | Size | Duration | Difficulty |
|--------|------|----------|------------|
| `01_puppet_setup.md` | 2.1 KB | 5 min | Easy |
| `02_puppet_parsing.md` | 5.9 KB | 10 min | Easy |
| `03_module_dependencies.md` | 7.4 KB | 15 min | Medium |
| `04_security_scanning.md` | 6.8 KB | 10 min | Easy |
| `05_graph_queries.md` | 7.2 KB | 15 min | Medium |
| `06_cli_commands.md` | 6.5 KB | 10 min | Easy |
| `07_mcp_integration.md` | 5.8 KB | 15 min | Advanced |

**Total**: ~42 KB, ~80 minutes for complete testing

**Purpose**: Step-by-step instructions for executing UAT

---

## Test Data (7 files)

| File | Size | Purpose |
|------|------|---------|
| `test-data/README.md` | 3.2 KB | Test data documentation |
| `test-data/puppet-sample/modules/webserver/metadata.json` | 0.6 KB | Webserver module metadata |
| `test-data/puppet-sample/modules/webserver/manifests/init.pp` | 1.8 KB | Webserver manifest (with security issues) |
| `test-data/puppet-sample/modules/database/metadata.json` | 0.4 KB | Database module metadata |
| `test-data/puppet-sample/modules/database/manifests/init.pp` | 1.5 KB | Database manifest (clean) |
| `test-data/puppet-sample/modules/common/metadata.json` | 0.3 KB | Common module metadata |
| `test-data/puppet-sample/modules/common/manifests/init.pp` | 0.5 KB | Common manifest (clean) |

**Total**: ~8.3 KB, 3 Puppet modules

**Purpose**: Sample Puppet code for testing with known properties:
- webserver: Contains 3 CWE vulnerabilities
- database: Clean code, complex features
- common: Clean code, simple base module

---

## Expected Results (4 files)

| File | Size | Purpose |
|------|------|---------|
| `expected-results/parsing_output.txt` | 2.1 KB | Expected parsing results |
| `expected-results/dependency_graph.txt` | 2.8 KB | Expected dependency analysis |
| `expected-results/security_findings.json` | 0.9 KB | Expected security scan results |
| `expected-results/query_results.txt` | 3.4 KB | Expected graph query results |

**Total**: ~9.2 KB

**Purpose**: Reference outputs for validating test results

---

## File Organization

```
user-acceptance-tests/
├── README.md                          # Start here
├── UAT_PLAN.md                        # Detailed plan
├── ACCEPTANCE_CHECKLIST.md            # Sign-off form
├── TESTING_SUMMARY.md                 # Quick reference
├── MANIFEST.md                        # This file
│
├── test-scripts/                      # Step-by-step tests
│   ├── 01_puppet_setup.md
│   ├── 02_puppet_parsing.md
│   ├── 03_module_dependencies.md
│   ├── 04_security_scanning.md
│   ├── 05_graph_queries.md
│   ├── 06_cli_commands.md
│   └── 07_mcp_integration.md
│
├── test-data/                         # Sample Puppet code
│   ├── README.md
│   └── puppet-sample/
│       └── modules/
│           ├── webserver/
│           │   ├── metadata.json
│           │   └── manifests/init.pp
│           ├── database/
│           │   ├── metadata.json
│           │   └── manifests/init.pp
│           └── common/
│               ├── metadata.json
│               └── manifests/init.pp
│
└── expected-results/                  # Reference outputs
    ├── parsing_output.txt
    ├── dependency_graph.txt
    ├── security_findings.json
    └── query_results.txt
```

---

## Usage Workflow

1. **Review** → Read `README.md`
2. **Plan** → Read `UAT_PLAN.md`
3. **Execute** → Follow `test-scripts/01-07` in order
4. **Validate** → Compare results with `expected-results/`
5. **Sign-Off** → Complete `ACCEPTANCE_CHECKLIST.md`
6. **Summarize** → Review `TESTING_SUMMARY.md`

---

## Testing Coverage

### Features Tested

- [x] Puppet manifest parsing (.pp files)
- [x] Module metadata parsing (metadata.json)
- [x] Module dependency graphs
- [x] Security vulnerability scanning (3 CWE patterns)
- [x] Graph node/edge indexing
- [x] CLI commands (3 commands, 12+ flags)
- [x] MCP integration (optional)

### Test Types

- [x] Functional testing (all features work)
- [x] Performance testing (< 5s for 100 files)
- [x] Error handling (graceful failures)
- [x] Usability testing (CLI clarity)
- [x] Integration testing (graph + CLI + MCP)
- [x] Security testing (CWE detection)

---

## Quality Metrics

| Metric | Target | How to Measure |
|--------|--------|----------------|
| **Test Coverage** | 100% of features | All scripts pass |
| **Accuracy** | > 95% | Compare with expected results |
| **Performance** | < 5s build time | Time graph initialization |
| **Security Detection** | 100% of CWEs | All 3 patterns found |
| **False Positives** | < 10% | Check clean modules |
| **Usability** | 4/5 rating | User feedback in checklist |

---

## Maintenance

### Updating Test Data

When Puppet support changes:

1. Update test-data manifests if needed
2. Re-generate expected-results files
3. Update test scripts with new steps
4. Increment UAT version number

### Adding New Tests

To add a new test script:

1. Create `test-scripts/08_new_feature.md`
2. Add to `README.md` test list
3. Add expected results if applicable
4. Update `ACCEPTANCE_CHECKLIST.md`

---

## Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-18 | Initial UAT package for Phase 18 Puppet support |

---

## Dependencies

**To run UAT**:
- rBuilder compiled with `--features bundle-extended`
- Test fixtures in `tests/fixtures/puppet/` (already present)
- Terminal access
- ~60 minutes

**Optional**:
- MCP server feature for Script 07
- Claude Code for MCP testing
- jq for JSON validation

---

## Deliverables Checklist

When UAT is complete, you should have:

- [x] All test scripts executed
- [x] Results documented in checklist
- [x] Performance metrics recorded
- [x] Issues logged (if any)
- [x] Sign-off signatures obtained
- [x] Recommendation made (Accept/Reject)

---

**Document Maintainer**: QA Team  
**Last Updated**: June 18, 2026  
**UAT Version**: 1.0
