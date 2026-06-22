# User Acceptance Test Plan - Puppet Support (Phase 18)

**Document Version**: 1.0  
**Date**: June 18, 2026  
**Feature**: Puppet Module and Manifest Analysis  
**Test Environment**: Development/Staging  
**Estimated Duration**: 60 minutes

---

## 1. Executive Summary

### Purpose

Validate that rBuilder's Puppet support (Phase 18) meets user requirements for:
- Parsing Puppet manifests and metadata
- Building module dependency graphs
- Detecting security vulnerabilities
- Providing CLI and MCP interfaces
- Integrating with the knowledge graph

### Scope

**In Scope**:
- ✅ Puppet manifest (`.pp`) parsing
- ✅ Module metadata (`metadata.json`) parsing
- ✅ Dependency graph construction
- ✅ Security scanning (CWE-78, CWE-798, CWE-732)
- ✅ CLI commands (`modules`, `validate`, `security-scan`)
- ✅ Graph queries for Puppet nodes/edges
- ✅ MCP tool integration

**Out of Scope**:
- ❌ Puppet catalog compilation
- ❌ PuppetDB integration
- ❌ Runtime manifest evaluation
- ❌ Hiera data resolution
- ❌ Puppet 7+ features (Bolt, etc.)

### Success Criteria

UAT passes if:
- [x] All 7 test scripts complete successfully
- [x] No critical or major bugs found
- [x] Performance meets targets (< 5s for 100 manifests)
- [x] Security scanner detects known vulnerabilities
- [x] Documentation is clear and accurate
- [x] User experience is smooth and intuitive

---

## 2. Test Objectives

### Primary Objectives

| ID | Objective | Priority | Success Metric |
|----|-----------|----------|----------------|
| **OBJ-01** | Validate manifest parsing accuracy | Critical | 100% symbol extraction |
| **OBJ-02** | Verify dependency graph correctness | Critical | Correct topological sort |
| **OBJ-03** | Confirm security pattern detection | Critical | All CWE patterns found |
| **OBJ-04** | Ensure CLI usability | High | Commands work without errors |
| **OBJ-05** | Test graph integration | High | Queries return correct nodes |
| **OBJ-06** | Validate MCP functionality | Medium | Tools callable from agents |
| **OBJ-07** | Check documentation accuracy | Medium | No contradictions found |

### Secondary Objectives

- Performance benchmarking
- Error message clarity
- Output format consistency
- Edge case handling

---

## 3. Test Environment

### Hardware Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| **CPU** | 2 cores | 4+ cores |
| **RAM** | 4 GB | 8+ GB |
| **Disk** | 1 GB free | 5+ GB free |
| **OS** | macOS 12+ | macOS 14+ |

### Software Requirements

| Software | Version | Required |
|----------|---------|----------|
| **Rust** | 1.70+ | Yes |
| **rBuilder** | Latest | Yes |
| **Puppet** | N/A | No (parser only) |
| **Git** | 2.0+ | Yes |
| **jq** | 1.6+ | Optional (JSON parsing) |

### Build Configuration

```bash
cargo build --release --features bundle-extended
```

Features required:
- `lang-puppet` (included in `bundle-extended`)
- `mcp-server` (optional, for test script 07)

---

## 4. Test Data

### Test Modules

Located in `test-data/puppet-sample/modules/`:

#### Module 1: webserver
- **Purpose**: Complex module with security issues
- **Files**:
  - `metadata.json` - Dependencies on `common`, `firewall`
  - `manifests/init.pp` - Main class with resources
  - `manifests/config.pp` - Configuration class
  - `manifests/service.pp` - Service management
- **Security Issues**:
  - CWE-78: Unquoted variable in `exec`
  - CWE-798: Hardcoded password
  - CWE-732: World-writable file (0777)

#### Module 2: database
- **Purpose**: Database module with dependencies
- **Files**:
  - `metadata.json` - Dependencies on `common`
  - `manifests/init.pp` - PostgreSQL setup
  - `manifests/backup.pp` - Backup configuration
- **Features**:
  - Fact usage (`$facts['os']['family']`)
  - Resource relationships (require, notify)

#### Module 3: common
- **Purpose**: Base dependency module
- **Files**:
  - `metadata.json` - No dependencies (leaf node)
  - `manifests/init.pp` - Common utilities
  - `manifests/packages.pp` - Package management
- **Features**:
  - Clean code (no security issues)
  - Simple structure

### Dependency Structure

```
webserver → common
webserver → firewall
database  → common
```

**Expected topological sort**: `common, firewall, database, webserver`

---

## 5. Test Scripts

### Script 01: Setup and Verification

**File**: `test-scripts/01_puppet_setup.md`

**Objectives**:
- Verify rBuilder is installed correctly
- Check Puppet feature flag is enabled
- Validate test data is present

**Duration**: 5 minutes

**Pass Criteria**:
- `rbuilder --version` succeeds
- `rbuilder --help` shows `puppet` subcommand
- Test data directory exists

### Script 02: Puppet Parsing

**File**: `test-scripts/02_puppet_parsing.md`

**Objectives**:
- Parse Puppet manifests and extract symbols
- Verify class, resource, and variable extraction
- Test metadata.json parsing

**Duration**: 10 minutes

**Pass Criteria**:
- All 3 modules parse without errors
- Symbol counts match expected values
- Relations are extracted correctly

### Script 03: Module Dependencies

**File**: `test-scripts/03_module_dependencies.md`

**Objectives**:
- Build module dependency graph
- Perform topological sort
- Detect circular dependencies

**Duration**: 15 minutes

**Pass Criteria**:
- Dependency graph has 3 modules
- Topological sort is correct
- No false circular dependency warnings

### Script 04: Security Scanning

**File**: `test-scripts/04_security_scanning.md`

**Objectives**:
- Detect CWE-78 (command injection)
- Detect CWE-798 (hardcoded secrets)
- Detect CWE-732 (file permissions)
- Test severity filtering

**Duration**: 10 minutes

**Pass Criteria**:
- All 3 CWE patterns detected in webserver module
- Severity filtering works correctly
- JSON output is valid

### Script 05: Graph Queries

**File**: `test-scripts/05_graph_queries.md`

**Objectives**:
- Index Puppet modules into graph
- Query for node types
- Query for edge types
- Test custom GQL queries

**Duration**: 15 minutes

**Pass Criteria**:
- Graph contains PuppetModule nodes
- Graph contains PuppetClass nodes
- Edge relationships are correct

### Script 06: CLI Commands

**File**: `test-scripts/06_cli_commands.md`

**Objectives**:
- Test `puppet modules` command
- Test `puppet validate` command
- Test `puppet security-scan` command
- Test output formats (text, json, mermaid)

**Duration**: 10 minutes

**Pass Criteria**:
- All commands execute successfully
- Output formats are valid
- Error messages are clear

### Script 07: MCP Integration

**File**: `test-scripts/07_mcp_integration.md`

**Objectives**:
- Start MCP server
- Call Puppet analysis tools
- Verify tool responses

**Duration**: 15 minutes

**Pass Criteria**:
- MCP server starts without errors
- Tools are listed in capabilities
- Tool calls return valid responses

---

## 6. Test Execution

### Execution Order

Tests must be run in sequence:

1. **Setup** (Script 01) - Required for all tests
2. **Parsing** (Script 02) - Required for scripts 3-7
3. **Dependencies** (Script 03) - Independent
4. **Security** (Script 04) - Independent
5. **Queries** (Script 05) - Requires graph init
6. **CLI** (Script 06) - Independent
7. **MCP** (Script 07) - Optional

### Test Roles

| Role | Responsibilities |
|------|------------------|
| **Test Executor** | Runs test scripts, records results |
| **Test Reviewer** | Validates results against expected outputs |
| **Issue Reporter** | Documents and reports failures |

### Execution Schedule

**Day 1**: Scripts 01-03 (Setup, Parsing, Dependencies)  
**Day 2**: Scripts 04-06 (Security, Queries, CLI)  
**Day 3**: Script 07 (MCP Integration) + Review

Total: 3 days for thorough testing (or 1 day for quick validation)

---

## 7. Pass/Fail Criteria

### Overall Pass Criteria

✅ UAT passes if **ALL** of the following are met:

1. **Functional Correctness**: All test scripts pass
2. **Security**: All CWE patterns detected
3. **Performance**: Parse 100 manifests in < 5 seconds
4. **Stability**: No crashes or hangs
5. **Documentation**: Matches actual behavior
6. **User Experience**: Commands are intuitive

### Individual Test Pass Criteria

| Test | Pass Criteria | Fail Criteria |
|------|---------------|---------------|
| **01 Setup** | Version shows, help works | Command not found |
| **02 Parsing** | Symbols extracted correctly | Parse errors, missing symbols |
| **03 Dependencies** | Graph is correct | Wrong order, false cycles |
| **04 Security** | All CWE found | Missed vulnerabilities |
| **05 Queries** | Correct nodes returned | Empty results, wrong types |
| **06 CLI** | All commands work | Crashes, hangs |
| **07 MCP** | Tools callable | Server won't start |

### Severity Classification

| Severity | Definition | UAT Impact |
|----------|------------|------------|
| **Critical** | Feature doesn't work at all | ❌ Fail UAT |
| **Major** | Feature works but with significant issues | ⚠️ Conditional pass |
| **Minor** | Small issues, workarounds exist | ✅ Pass with notes |
| **Cosmetic** | UI/text issues only | ✅ Pass |

---

## 8. Issue Tracking

### Issue Template

```markdown
**Issue ID**: UAT-PUPPET-XXX
**Severity**: [Critical/Major/Minor/Cosmetic]
**Test Script**: [Script number and name]
**Step**: [Specific step that failed]

**Description**:
[What went wrong]

**Expected**:
[What should have happened]

**Actual**:
[What actually happened]

**Reproduction**:
1. [Step 1]
2. [Step 2]
...

**Environment**:
- OS: [macOS 14.5]
- Rust: [1.75.0]
- rBuilder: [commit hash]

**Logs**:
```
[Error output]
```

**Screenshots**: [If applicable]
```

### Issue Workflow

1. **Discovery** → Log issue with template
2. **Triage** → Assign severity
3. **Investigation** → Reproduce and diagnose
4. **Resolution** → Fix or document workaround
5. **Verification** → Re-test
6. **Closure** → Update checklist

---

## 9. Test Metrics

### Metrics to Collect

| Metric | Target | How to Measure |
|--------|--------|----------------|
| **Parse Speed** | < 50ms per manifest | Time `puppet validate` |
| **Graph Build** | < 5s for 100 files | Time `rbuilder init` |
| **Memory Usage** | < 500MB | Monitor with `top` |
| **Security Recall** | 100% of known issues | Count findings |
| **False Positives** | < 10% | Manual review |
| **User Satisfaction** | Positive feedback | Survey |

### Data Collection

```bash
# Performance metrics
time rbuilder puppet modules ./test-data/puppet-sample --show-deps

# Memory usage
/usr/bin/time -l rbuilder init ./test-data/puppet-sample

# Security metrics
rbuilder puppet security-scan ./test-data/puppet-sample --format json | \
  jq '.[] | .cwe' | sort | uniq -c
```

---

## 10. Acceptance Criteria

### Feature Acceptance

Each feature must meet these criteria:

#### Parsing
- ✅ Extracts classes from `.pp` files
- ✅ Extracts resources with attributes
- ✅ Extracts variables and facts
- ✅ Parses `metadata.json` dependencies
- ✅ Handles malformed input gracefully

#### Dependency Analysis
- ✅ Builds correct dependency graph
- ✅ Performs topological sort
- ✅ Detects circular dependencies
- ✅ Shows dependency trees

#### Security Scanning
- ✅ Detects CWE-78 (command injection)
- ✅ Detects CWE-798 (secrets)
- ✅ Detects CWE-732 (permissions)
- ✅ Filters by severity
- ✅ Provides remediation guidance

#### CLI Interface
- ✅ Commands have consistent flags
- ✅ Help text is clear
- ✅ Error messages are actionable
- ✅ Output formats work (text, json, mermaid)

#### Graph Integration
- ✅ Nodes indexed correctly
- ✅ Edges represent relationships
- ✅ Queries return expected results
- ✅ Performance is acceptable

### Documentation Acceptance

- ✅ `docs/puppet_support.md` is accurate
- ✅ README examples work
- ✅ CLI help text matches docs
- ✅ No broken links
- ✅ Examples are copy-pasteable

### Sign-Off Criteria

UAT is complete when:

1. ✅ All 7 test scripts pass
2. ✅ No critical or major issues remain
3. ✅ Performance targets met
4. ✅ `ACCEPTANCE_CHECKLIST.md` is 100% complete
5. ✅ Test executor and reviewer sign off

---

## 11. Risks and Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| **Test data too simple** | Medium | Low | Use real-world Puppet code |
| **Performance on large repos** | High | Medium | Test with 1000+ manifests |
| **Platform-specific issues** | Medium | Medium | Test on Linux/Windows |
| **Regex parsing limitations** | High | Low | Compare with tree-sitter |
| **Security false positives** | Medium | Medium | Manual review of findings |

---

## 12. Test Deliverables

### Required Deliverables

1. ✅ **Completed test scripts** (7 scripts)
2. ✅ **Test execution results** (pass/fail per script)
3. ✅ **Issue log** (if any failures)
4. ✅ **Performance metrics** (timing data)
5. ✅ **Acceptance checklist** (signed off)
6. ✅ **UAT summary report** (this document)

### Optional Deliverables

- Screenshots of successful tests
- Video walkthrough of testing process
- Comparison with Puppet linting tools
- Performance benchmarks

---

## 13. Appendix

### A. Glossary

- **UAT**: User Acceptance Testing
- **CWE**: Common Weakness Enumeration
- **MCP**: Model Context Protocol
- **GQL**: Graph Query Language
- **PDG**: Program Dependence Graph

### B. References

- [Puppet Language Specification](https://puppet.com/docs/puppet/latest/lang_summary.html)
- [OWASP Top 10](https://owasp.org/www-project-top-ten/)
- [CWE Database](https://cwe.mitre.org/)
- [rBuilder Documentation](../README.md)

### C. Version History

| Version | Date | Changes |
|---------|------|---------|
| 1.0 | 2026-06-18 | Initial UAT plan |

---

**Document Owner**: QA Team  
**Approver**: Product Manager  
**Next Review**: 2026-12-18
