# Test Script 07: MCP Integration

**Test ID**: UAT-PUPPET-07  
**Duration**: 15 minutes  
**Difficulty**: Advanced  
**Prerequisites**: Scripts 01-02 passed, MCP server feature enabled

---

## Objective

Validate that Puppet analysis tools are accessible via Model Context Protocol (MCP) for AI agents like Claude Code.

---

## Prerequisites Check

Before starting, verify MCP server is available:

```bash
cd ~/git/rust/rBuilder

# Check if MCP server feature is compiled
./target/release/rbuilder --help | grep "mcp"
```

**Expected**: `mcp` subcommand listed

**If not available**: Rebuild with MCP support:
```bash
cargo build --release --features bundle-extended,mcp-server
```

---

## Test Steps

### Step 1: Start MCP Server

**Action**: Start rBuilder MCP server in stdio mode

```bash
# In Terminal 1
./target/release/rbuilder mcp serve --transport stdio
```

**Expected Output**:
```
MCP Server starting in stdio mode...
Ready to accept connections
```

**Pass Criteria**:
- ✅ Server starts without errors
- ✅ No crashes
- ✅ Waits for input

**Fail Criteria**:
- ❌ Server won't start
- ❌ Immediate crash
- ❌ Port conflict errors

**What's Being Tested**: MCP server initialization

---

### Step 2: Test Server Capabilities (Manual)

**Action**: Send capabilities request via JSON-RPC

In a new terminal (Terminal 2):

```bash
# Send JSON-RPC request
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | \
  ./target/release/rbuilder mcp serve --transport stdio
```

**Expected Output** (JSON response):
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "tools": [
      {
        "name": "analyze_puppet_module",
        "description": "Analyze a Puppet module and extract classes, resources, and dependencies",
        "inputSchema": {
          "type": "object",
          "properties": {
            "module_path": {"type": "string"}
          }
        }
      },
      {
        "name": "find_puppet_classes",
        "description": "Find all Puppet classes in the knowledge graph",
        "inputSchema": {
          "type": "object",
          "properties": {
            "pattern": {"type": "string"}
          }
        }
      },
      {
        "name": "puppet_security_scan",
        "description": "Scan Puppet modules for security vulnerabilities",
        "inputSchema": {
          "type": "object",
          "properties": {
            "module_path": {"type": "string"},
            "min_severity": {"type": "string"}
          }
        }
      }
    ]
  }
}
```

**Pass Criteria**:
- ✅ Valid JSON-RPC response
- ✅ Puppet tools listed
- ✅ Tool schemas included

**Fail Criteria**:
- ❌ Invalid JSON
- ❌ No Puppet tools
- ❌ Missing schemas

**What's Being Tested**: MCP tool registration

---

### Step 3: Test Claude Code Integration

**Action**: Configure Claude Code to use rBuilder MCP server

**Setup**:
1. Edit `~/.claude/mcp_servers.json`:
   ```json
   {
     "rbuilder-puppet-test": {
       "command": "/Users/sshaaf/git/rust/rBuilder/target/release/rbuilder",
       "args": ["mcp", "serve", "--transport", "stdio"],
       "cwd": "/Users/sshaaf/git/rust/rBuilder/tests/fixtures/puppet"
     }
   }
   ```

2. Restart Claude Code

3. In Claude Code, type:
   ```
   Use the rbuilder MCP server to analyze Puppet modules
   ```

**Expected Behavior**:
- ✅ Claude recognizes rbuilder tools
- ✅ Can call analyze_puppet_module
- ✅ Returns module information

**Pass Criteria**:
- ✅ Tools are available in Claude
- ✅ Tool calls succeed
- ✅ Results are accurate

**Fail Criteria**:
- ❌ Claude can't see tools
- ❌ Tool calls fail
- ❌ Server crashes on call

**What's Being Tested**: End-to-end MCP integration with AI agent

---

### Step 4: Test `analyze_puppet_module` Tool

**Action**: Call the module analysis tool via MCP

**In Claude Code**:
```
Analyze the nginx Puppet module using the rbuilder tool
```

**Expected Response**:
```
The nginx module contains:
- 1 main class (nginx)
- 4 resources (package, service, file, exec)
- Dependencies: common
- Version: 1.0.0
```

**Pass Criteria**:
- ✅ Module analyzed correctly
- ✅ Classes found
- ✅ Resources found
- ✅ Dependencies identified

**Fail Criteria**:
- ❌ Tool call fails
- ❌ Incomplete analysis
- ❌ Wrong data returned

**What's Being Tested**: Module analysis via MCP

---

### Step 5: Test `find_puppet_classes` Tool

**Action**: Find classes matching a pattern

**In Claude Code**:
```
Find all Puppet classes with "nginx" in the name
```

**Expected Response**:
```
Found 2 classes:
1. class::nginx
2. class::nginx::base
```

**Pass Criteria**:
- ✅ Query succeeds
- ✅ Pattern matching works
- ✅ Correct results returned

**Fail Criteria**:
- ❌ No results
- ❌ Wrong classes returned

**What's Being Tested**: Graph query via MCP

---

### Step 6: Test `puppet_security_scan` Tool

**Action**: Run security scan via MCP

**In Claude Code**:
```
Scan the nginx Puppet module for security issues
```

**Expected Response**:
```
Found 3 security issues:

1. [Critical] Command injection in exec resource 'reload'
   CWE: CWE-78
   Fix: Use shellquote() for variables

2. [High] Hardcoded secret in file resource
   CWE: CWE-798
   Fix: Use Hiera lookup()

3. [Medium] Insecure file permissions (0666)
   CWE: CWE-732
   Fix: Use restrictive modes
```

**Pass Criteria**:
- ✅ All 3 issues found
- ✅ Severity correct
- ✅ Remediation provided

**Fail Criteria**:
- ❌ No issues found
- ❌ Wrong CWEs
- ❌ Tool crashes

**What's Being Tested**: Security scanning via MCP

---

### Step 7: Test Error Handling

**Action**: Call tool with invalid parameters

**In Claude Code**:
```
Analyze a Puppet module at /nonexistent/path
```

**Expected Response**:
```
Error: Module not found at /nonexistent/path
```

**Pass Criteria**:
- ✅ Error message returned
- ✅ Server doesn't crash
- ✅ Error is clear

**Fail Criteria**:
- ❌ Server crashes
- ❌ Unclear error
- ❌ Hangs indefinitely

**What's Being Tested**: MCP error handling

---

### Step 8: Test Concurrent Requests

**Action**: Make multiple tool calls in quick succession

**In Claude Code**:
```
1. Find all Puppet classes
2. Scan for security issues
3. Analyze module dependencies
```

**Pass Criteria**:
- ✅ All requests handled
- ✅ No race conditions
- ✅ Correct responses for each

**Fail Criteria**:
- ❌ Server crashes
- ❌ Responses mixed up
- ❌ Timeouts

**What's Being Tested**: MCP server concurrency

---

### Step 9: Test Server Shutdown

**Action**: Stop the MCP server gracefully

```bash
# In Terminal 1 (where server is running)
# Press Ctrl+C

# Or send shutdown signal
kill -TERM <pid>
```

**Expected Output**:
```
Shutting down MCP server...
Server stopped gracefully
```

**Pass Criteria**:
- ✅ Server stops cleanly
- ✅ No resource leaks
- ✅ No error messages

**Fail Criteria**:
- ❌ Hangs on shutdown
- ❌ Leaves processes running
- ❌ Errors on exit

**What's Being Tested**: Graceful shutdown

---

### Step 10: Test Server Restart

**Action**: Restart server and verify it works

```bash
# Start again
./target/release/rbuilder mcp serve --transport stdio
```

**Pass Criteria**:
- ✅ Restarts successfully
- ✅ Tools still available
- ✅ No state issues

**Fail Criteria**:
- ❌ Won't restart
- ❌ Tools missing
- ❌ Corruption

**What's Being Tested**: Server reliability

---

## Test Summary

### MCP Tools Tested

| Tool | Tested | Works | Status |
|------|--------|-------|--------|
| `analyze_puppet_module` | ⬜ | ⬜ | ⬜ |
| `find_puppet_classes` | ⬜ | ⬜ | ⬜ |
| `puppet_security_scan` | ⬜ | ⬜ | ⬜ |

### Integration Points

| Integration | Status | Notes |
|-------------|--------|-------|
| MCP server starts | ⬜ | |
| Tools registered | ⬜ | |
| Claude Code connection | ⬜ | |
| Tool calls succeed | ⬜ | |
| Error handling works | ⬜ | |

### Checklist

- [ ] Step 1: Server starts successfully
- [ ] Step 2: Capabilities list correctly
- [ ] Step 3: Claude Code integration works
- [ ] Step 4: Module analysis tool works
- [ ] Step 5: Class finder tool works
- [ ] Step 6: Security scan tool works
- [ ] Step 7: Error handling is graceful
- [ ] Step 8: Concurrent requests handled
- [ ] Step 9: Shutdown is clean
- [ ] Step 10: Restart works

### Result

**Overall Status**: ⬜ Not Run / ✅ Pass / ❌ Fail / 🟡 Optional

**MCP Functionality**: _____ / 10 steps passed

**Notes**:
```
[Record any observations]

Note: This test is OPTIONAL. If MCP server feature is not enabled
or Claude Code is not available, mark as 🟡 Optional and skip.
```

### Performance Metrics

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| Server startup time | | < 1s | ⬜ |
| Tool call latency | | < 500ms | ⬜ |
| Memory usage | | < 100MB | ⬜ |
| Shutdown time | | < 2s | ⬜ |

### Issues Found

| Step | Issue | Severity |
|------|-------|----------|
| - | - | - |

### Next Steps

If all checks pass: ✅ **MCP Integration Validated**

If MCP is not available: 🟡 **Mark as Optional - Not Required for UAT**

If any check fails:
1. Review MCP server configuration
2. Check Claude Code mcp_servers.json
3. Verify tool registration
4. Check server logs
5. Re-run failed steps

---

## Cleanup

After testing, remove the test MCP server config:

```bash
# Edit ~/.claude/mcp_servers.json and remove "rbuilder-puppet-test" entry
```

---

**Test Executed By**: _______________  
**Date**: _______________  
**Signature**: _______________  
**Skipped Reason** (if applicable): _______________
