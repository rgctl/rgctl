# User Acceptance Testing - Phase 18 (Puppet Support)

**Version**: 1.0  
**Date**: June 18, 2026  
**Feature**: Puppet module and manifest analysis

---

## Overview

This directory contains comprehensive user acceptance tests for rBuilder's Puppet support (Phase 18). Follow these tests to validate that Puppet analysis features work correctly in your environment.

## What's Being Tested

- ✅ Puppet manifest parsing (`.pp` files)
- ✅ Module metadata extraction (`metadata.json`)
- ✅ Dependency graph analysis
- ✅ Security vulnerability scanning
- ✅ CLI commands
- ✅ Graph queries
- ✅ MCP integration (optional)

## Prerequisites

Before starting, ensure you have:

- [ ] rBuilder compiled with Puppet support (`--features bundle-extended`)
- [ ] Basic familiarity with Puppet manifests
- [ ] 30-60 minutes for complete testing
- [ ] Terminal/command line access

## Quick Start

```bash
# 1. Navigate to this directory
cd user-acceptance-tests

# 2. Run the test scripts in order
./test-scripts/01_puppet_setup.md
./test-scripts/02_puppet_parsing.md
# ... continue with remaining scripts

# 3. Check off items in ACCEPTANCE_CHECKLIST.md
```

## Test Structure

| Script | Duration | Difficulty | Prerequisites |
|--------|----------|------------|---------------|
| **01_puppet_setup** | 5 min | Easy | None |
| **02_puppet_parsing** | 10 min | Easy | Script 01 |
| **03_module_dependencies** | 15 min | Medium | Script 02 |
| **04_security_scanning** | 10 min | Easy | Script 02 |
| **05_graph_queries** | 15 min | Medium | Script 02 |
| **06_cli_commands** | 10 min | Easy | Script 02 |
| **07_mcp_integration** | 15 min | Advanced | MCP server setup |

**Total Time**: ~60 minutes for complete testing

## Files in This Directory

```
user-acceptance-tests/
├── README.md                      # This file
├── UAT_PLAN.md                    # Detailed test plan
├── ACCEPTANCE_CHECKLIST.md        # Sign-off checklist
├── test-scripts/                  # Step-by-step test instructions
│   ├── 01_puppet_setup.md
│   ├── 02_puppet_parsing.md
│   ├── 03_module_dependencies.md
│   ├── 04_security_scanning.md
│   ├── 05_graph_queries.md
│   ├── 06_cli_commands.md
│   └── 07_mcp_integration.md
├── test-data/                     # Sample Puppet code
│   └── puppet-sample/             # Test fixtures
│       └── modules/
│           ├── webserver/
│           ├── database/
│           └── common/
└── expected-results/              # Expected outputs
    ├── parsing_output.txt
    ├── dependency_graph.txt
    └── security_findings.json
```

## How to Use This Guide

### For First-Time Users

1. **Read UAT_PLAN.md** to understand testing objectives
2. **Follow test scripts sequentially** (01 → 07)
3. **Compare your results** with files in `expected-results/`
4. **Check off items** in `ACCEPTANCE_CHECKLIST.md`
5. **Report issues** if anything fails

### For Experienced Users

- Jump to specific test scripts based on features you want to validate
- Use `test-data/puppet-sample/` as reference examples
- Adapt tests to your own Puppet repositories

### For Automated Testing

```bash
# Run all tests programmatically
for script in test-scripts/*.md; do
    echo "Running $script..."
    # Extract and execute code blocks
done
```

## Test Data

Pre-built Puppet modules are provided in `test-data/puppet-sample/`:

- **webserver** - Nginx module with security issues (intentional)
- **database** - PostgreSQL module with dependencies
- **common** - Shared utilities module

These modules intentionally include:
- Security vulnerabilities (for scanner testing)
- Circular dependencies (for validation testing)
- Complex manifests (for parser testing)

## Expected Results

Expected outputs for each test are in `expected-results/`:

- **parsing_output.txt** - Symbol extraction results
- **dependency_graph.txt** - Module dependency tree
- **security_findings.json** - Security scan results
- **query_results.txt** - Graph query outputs

## Pass/Fail Criteria

### ✅ Pass Criteria

Each test script defines specific pass criteria. Generally:
- Commands execute without errors
- Output matches expected format
- Security issues are detected
- Graph structure is correct

### ❌ Fail Criteria

Tests fail if:
- Commands crash or hang
- Output is malformed or empty
- Security issues are missed
- Graph has incorrect relationships

## Reporting Issues

If tests fail:

1. **Capture the error**:
   ```bash
   rbuilder puppet modules ./test-data/puppet-sample 2>&1 | tee error.log
   ```

2. **Check the checklist** for known issues

3. **Report to GitHub**:
   - Include error log
   - Specify which test script failed
   - Provide your environment (OS, Rust version)

## Getting Help

- **Documentation**: See `docs/puppet_support.md`
- **Examples**: Check `tests/fixtures/puppet/`
- **Issues**: https://github.com/sshaaf/rBuilder/issues

## Test Environment

These tests are validated on:
- **macOS** 14+ (Darwin 25.5.0)
- **Rust** 1.70+
- **rBuilder** with `bundle-extended` features

May work on Linux/Windows but not officially tested.

## Next Steps After Testing

1. ✅ Complete all test scripts
2. ✅ Fill out `ACCEPTANCE_CHECKLIST.md`
3. ✅ Sign off on UAT completion
4. 🚀 Use Puppet features in production!

---

**Questions?** See `UAT_PLAN.md` for detailed testing methodology.
