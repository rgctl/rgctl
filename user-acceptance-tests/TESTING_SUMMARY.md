# User Acceptance Testing - Summary

**Feature**: Puppet Support (Phase 18)  
**Version**: 1.0  
**Created**: June 18, 2026  
**Last Updated**: June 18, 2026

---

## Quick Reference

### Getting Started

1. **Review** → Read `README.md` for overview
2. **Plan** → Read `UAT_PLAN.md` for methodology
3. **Execute** → Run test scripts 01-07 in order
4. **Compare** → Check results against `expected-results/`
5. **Sign-Off** → Complete `ACCEPTANCE_CHECKLIST.md`

### Time Commitment

- **Quick validation**: ~30 minutes (scripts 01-04)
- **Full validation**: ~60 minutes (scripts 01-07)
- **With documentation**: ~90 minutes (includes review)

### What's Included

```
user-acceptance-tests/
├── README.md                    # Start here
├── UAT_PLAN.md                  # Detailed test plan
├── ACCEPTANCE_CHECKLIST.md      # Sign-off document
├── TESTING_SUMMARY.md           # This file
├── test-scripts/                # Step-by-step instructions
│   ├── 01_puppet_setup.md       # 5 min
│   ├── 02_puppet_parsing.md     # 10 min
│   ├── 03_module_dependencies.md # 15 min
│   ├── 04_security_scanning.md  # 10 min
│   ├── 05_graph_queries.md      # 15 min
│   ├── 06_cli_commands.md       # 10 min
│   └── 07_mcp_integration.md    # 15 min (optional)
├── test-data/                   # Sample Puppet modules
│   └── puppet-sample/
│       └── modules/
│           ├── webserver/       # With security issues
│           ├── database/        # Clean code
│           └── common/          # Base module
└── expected-results/            # Expected outputs
    ├── parsing_output.txt
    ├── dependency_graph.txt
    ├── security_findings.json
    └── query_results.txt
```

---

## Test Coverage

### Functional Tests

| Feature | Script | Status |
|---------|--------|--------|
| **Manifest Parsing** | 02 | ⬜ |
| **Metadata Parsing** | 02, 03 | ⬜ |
| **Dependency Graph** | 03 | ⬜ |
| **Security Scanning** | 04 | ⬜ |
| **Graph Indexing** | 05 | ⬜ |
| **CLI Commands** | 06 | ⬜ |
| **MCP Integration** | 07 (optional) | ⬜ |

### Non-Functional Tests

| Aspect | Tested In | Status |
|--------|-----------|--------|
| **Performance** | All scripts | ⬜ |
| **Error Handling** | Scripts 02, 04, 06 | ⬜ |
| **Usability** | Scripts 01, 06 | ⬜ |
| **Consistency** | Scripts 03, 06 | ⬜ |

---

## Test Data Summary

### Modules Provided

1. **webserver** (v2.1.0)
   - **Purpose**: Security testing
   - **Security Issues**: 3 (CWE-78, CWE-798, CWE-732)
   - **Complexity**: Medium
   - **Dependencies**: common, firewall

2. **database** (v1.5.2)
   - **Purpose**: Clean code testing
   - **Security Issues**: 0 (clean)
   - **Complexity**: Medium
   - **Dependencies**: common

3. **common** (v1.2.0)
   - **Purpose**: Base module
   - **Security Issues**: 0 (clean)
   - **Complexity**: Simple
   - **Dependencies**: None

### Key Statistics

| Metric | Value |
|--------|-------|
| Total Modules | 3 |
| Total Classes | 4 |
| Total Resources | 15+ |
| Total Variables | 3+ |
| Total Facts | 2+ |
| Dependency Edges | 3 |
| Security Issues | 3 (webserver only) |

---

## Expected Results Summary

### Parsing (Script 02)

- ✅ All 3 modules parse successfully
- ✅ 4 classes extracted
- ✅ 15+ resources extracted
- ✅ 3+ variables extracted
- ✅ 2+ facts extracted
- ✅ No parse errors

### Dependencies (Script 03)

- ✅ Correct dependency graph: webserver→common, database→common
- ✅ Valid topological sort: common, firewall, database, webserver
- ✅ No circular dependencies
- ✅ JSON and Mermaid outputs valid

### Security (Script 04)

- ✅ 3 vulnerabilities found in webserver module
- ✅ CWE-78 (critical): Command injection detected
- ✅ CWE-798 (high): Hardcoded secret detected
- ✅ CWE-732 (medium): Insecure permissions detected
- ✅ 0 false positives on clean modules

### Graph (Script 05)

- ✅ All node types indexed (Module, Class, Resource, Variable, Fact)
- ✅ All edge types present (DependsOn, Includes, Declares, etc.)
- ✅ Queries return correct results
- ✅ Metadata preserved

### CLI (Script 06)

- ✅ All 3 commands work (modules, validate, security-scan)
- ✅ All flags functional (--show-deps, --format, --from-graph, --min-severity)
- ✅ All formats valid (text, json, mermaid)
- ✅ Error handling is graceful

### MCP (Script 07 - Optional)

- ✅ Server starts without errors
- ✅ 3+ Puppet tools registered
- ✅ Tools callable from Claude Code
- ✅ Correct results returned

---

## Pass/Fail Criteria

### PASS Conditions

UAT passes if **ALL** of these are true:

1. ✅ All mandatory scripts (01-06) pass
2. ✅ Zero critical issues found
3. ✅ No more than 2 major issues found
4. ✅ Security scanner detects all 3 CWE patterns
5. ✅ Performance targets met (< 5s for graph build)
6. ✅ Acceptance checklist 100% complete

### FAIL Conditions

UAT fails if **ANY** of these are true:

1. ❌ Any mandatory script fails completely
2. ❌ Critical issues found (crashes, data loss, wrong results)
3. ❌ Security scanner misses vulnerabilities
4. ❌ Performance is unacceptable (> 10s for small repo)
5. ❌ Major functional gaps

### CONDITIONAL PASS

UAT can pass with conditions if:

- ⚠️ Minor issues found with documented workarounds
- ⚠️ Optional features not working (e.g., MCP)
- ⚠️ Cosmetic issues (typos, formatting)

---

## Common Issues & Troubleshooting

### Issue 1: "puppet: command not found"

**Cause**: Puppet feature not compiled  
**Fix**: Rebuild with `cargo build --release --features bundle-extended`

### Issue 2: Test data not found

**Cause**: Running from wrong directory  
**Fix**: `cd ~/git/rust/rBuilder` before running tests

### Issue 3: Graph not initialized

**Cause**: Skipped Script 02 graph initialization  
**Fix**: Run `rbuilder init tests/fixtures/puppet`

### Issue 4: MCP server won't start

**Cause**: MCP feature not compiled  
**Fix**: Rebuild with `--features mcp-server` or skip Script 07

### Issue 5: Performance too slow

**Cause**: Debug build instead of release  
**Fix**: Use `./target/release/rbuilder` not `./target/debug/rbuilder`

---

## Reporting Issues

If you find bugs during UAT:

1. **Document** the issue in `ACCEPTANCE_CHECKLIST.md`
2. **Capture** error output and logs
3. **Rate** severity (critical, major, minor, cosmetic)
4. **Report** to GitHub: https://github.com/sshaaf/rBuilder/issues

### Issue Template

```markdown
**UAT Issue**: [Brief description]
**Script**: [Which test script]
**Severity**: [Critical/Major/Minor/Cosmetic]

**Steps to Reproduce**:
1. [Step 1]
2. [Step 2]

**Expected**: [What should happen]
**Actual**: [What actually happened]

**Environment**:
- OS: [macOS 14.5]
- Rust: [1.75.0]
- rBuilder: [commit hash]
```

---

## Metrics to Track

### Functional Metrics

- **Parse Success Rate**: _____ %
- **Security Detection Rate**: _____ / 3 CWE patterns
- **Graph Coverage**: _____ % of expected nodes/edges
- **CLI Success Rate**: _____ % of commands working

### Performance Metrics

- **Parse Time**: _____ ms per manifest
- **Graph Build Time**: _____ s for 3 modules
- **Security Scan Time**: _____ ms
- **Query Time**: _____ ms average

### Quality Metrics

- **False Positives**: _____ (security scan)
- **False Negatives**: _____ (security scan)
- **Crashes**: _____ (should be 0)
- **Error Clarity**: _____ / 5 rating

---

## Final Deliverables

When UAT is complete, submit:

1. ✅ Completed `ACCEPTANCE_CHECKLIST.md` with signatures
2. ✅ Test execution logs (optional)
3. ✅ Issue list (if any failures)
4. ✅ Performance metrics
5. ✅ Recommendation (Accept/Accept with Conditions/Reject)

---

## Success Stories

### Example 1: Clean Modules Pass Security Scan

```bash
$ rbuilder puppet security-scan test-data/puppet-sample/modules/common
No security findings.
```

**Result**: ✅ No false positives - clean code is recognized as clean

### Example 2: Correct Topological Sort

```bash
$ rbuilder puppet modules test-data/puppet-sample/modules --show-deps
Dependency order:
  1. common
  2. firewall
  3. database
  4. webserver
```

**Result**: ✅ Dependencies are satisfied - common comes before its dependents

### Example 3: All CWE Patterns Detected

```bash
$ rbuilder puppet security-scan test-data/puppet-sample/modules/webserver
[Critical] ... CWE-78
[High] ... CWE-798
[Medium] ... CWE-732
```

**Result**: ✅ 100% detection rate - all intentional vulnerabilities found

---

## Frequently Asked Questions

### Q1: How long does UAT take?

**A**: 30-60 minutes for core features, 90 minutes including optional MCP testing.

### Q2: Can I skip Script 07 (MCP)?

**A**: Yes, MCP integration is optional. Mark as "Skipped" in checklist.

### Q3: What if I find a bug?

**A**: Document it in the checklist, capture logs, and report to GitHub.

### Q4: Can I use my own Puppet code?

**A**: Yes! The test data is just a baseline. Test with real-world code too.

### Q5: What's the minimum passing criteria?

**A**: All mandatory scripts (01-06) pass, zero critical issues, all 3 CWE patterns detected.

---

## Next Steps After UAT

### If Accepted

1. ✅ Merge Phase 18 to main branch
2. ✅ Tag release (e.g., v0.2.0)
3. ✅ Update documentation
4. ✅ Announce to users

### If Accepted with Conditions

1. ⚠️ Document known limitations
2. ⚠️ Create fix tickets
3. ⚠️ Schedule patch release
4. ⚠️ Merge with release notes

### If Rejected

1. ❌ Review critical issues
2. ❌ Fix and re-test
3. ❌ Schedule new UAT
4. ❌ Do not merge

---

## Resources

- **Documentation**: `docs/puppet_support.md`
- **Code Review**: `PHASE_18_PUPPET_CODE_REVIEW.md`
- **Test Fixtures**: `tests/fixtures/puppet/`
- **GitHub Issues**: https://github.com/sshaaf/rBuilder/issues

---

**UAT Lead**: _______________  
**UAT Date**: _______________  
**UAT Status**: ⬜ In Progress | ✅ Complete | ❌ Failed  
**Recommendation**: ⬜ Accept | ⬜ Accept with Conditions | ⬜ Reject
