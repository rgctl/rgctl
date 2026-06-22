# Test Script 04: Security Scanning

**Test ID**: UAT-PUPPET-04  
**Duration**: 10 minutes  
**Difficulty**: Easy  
**Prerequisites**: Scripts 01 and 02 passed

---

## Objective

Validate that security scanner detects CWE patterns in Puppet manifests: command injection, hardcoded secrets, and insecure file permissions.

---

## Test Steps

### Step 1: Scan Test Fixtures for Security Issues

**Action**: Run security scan on nginx module (contains intentional vulnerabilities)

```bash
cd ~/git/rust/rBuilder

./target/release/rbuilder puppet security-scan tests/fixtures/puppet/modules/nginx
```

**Expected Output**:
```
[Critical] Potential command injection in exec resource 'class::nginx::resource::exec::reload'
  CWE: CWE-78
  Fix: Use shellquote() for variable interpolation in commands

[High] Potential hardcoded secret in resource 'class::nginx::resource::file::/etc/nginx/nginx.conf'
  CWE: CWE-798
  Fix: Use Hiera lookup() or encrypted data instead

[Medium] Insecure file permissions in file resource 'class::nginx::resource::file::/etc/nginx/nginx.conf'
  CWE: CWE-732
  Fix: Use restrictive file modes (e.g. 0644 or 0600)
```

**Pass Criteria**:
- ✅ All 3 security findings detected
- ✅ CWE-78 (command injection) found
- ✅ CWE-798 (hardcoded secret) found
- ✅ CWE-732 (file permissions) found
- ✅ Remediation guidance provided for each

**Fail Criteria**:
- ❌ Zero findings (scanner not working)
- ❌ Missing any of the 3 CWE patterns
- ❌ False positives on clean code

**What's Being Tested**: Security pattern detection

---

### Step 2: Test Severity Filtering

**Action**: Filter findings by severity level

```bash
# Only show high and critical
./target/release/rbuilder puppet security-scan tests/fixtures/puppet/modules/nginx \
  --min-severity high
```

**Expected Output**:
```
[Critical] Potential command injection in exec resource...
  CWE: CWE-78

[High] Potential hardcoded secret in resource...
  CWE: CWE-798
```

**Pass Criteria**:
- ✅ Only 2 findings shown (critical + high)
- ✅ Medium finding (CWE-732) is filtered out
- ✅ Severity labels are correct

**Fail Criteria**:
- ❌ Medium findings still shown
- ❌ Wrong filtering

**What's Being Tested**: Severity filtering

---

### Step 3: Test JSON Output

**Action**: Get findings as JSON

```bash
./target/release/rbuilder puppet security-scan tests/fixtures/puppet/modules/nginx \
  --format json > /tmp/puppet_security.json

# Pretty-print
cat /tmp/puppet_security.json | jq '.'
```

**Expected Output**:
```json
[
  {
    "severity": "critical",
    "message": "Potential command injection in exec resource...",
    "location": "class::nginx::resource::exec::reload",
    "cwe": "CWE-78",
    "remediation": "Use shellquote() for variable interpolation in commands",
    "resource_type": "exec"
  },
  {
    "severity": "high",
    "message": "Potential hardcoded secret in resource...",
    "location": "class::nginx::resource::file::/etc/nginx/nginx.conf",
    "cwe": "CWE-798",
    "remediation": "Use Hiera lookup() or encrypted data instead",
    "resource_type": null
  },
  {
    "severity": "medium",
    "message": "Insecure file permissions in file resource...",
    "location": "class::nginx::resource::file::/etc/nginx/nginx.conf",
    "cwe": "CWE-732",
    "remediation": "Use restrictive file modes (e.g. 0644 or 0600)",
    "resource_type": "file"
  }
]
```

**Pass Criteria**:
- ✅ Valid JSON array
- ✅ All required fields present (severity, message, cwe, remediation)
- ✅ Severity values match enum (lowercase)

**Fail Criteria**:
- ❌ Invalid JSON
- ❌ Missing fields
- ❌ Empty array

**What's Being Tested**: JSON serialization of security findings

---

### Step 4: Test Clean Module (No Findings)

**Action**: Scan common module (should have no security issues)

```bash
./target/release/rbuilder puppet security-scan tests/fixtures/puppet/modules/common
```

**Expected Output**:
```
No security findings.
```

**Pass Criteria**:
- ✅ Zero findings for clean code
- ✅ Message says "No security findings"
- ✅ Exit code is 0 (success)

**Fail Criteria**:
- ❌ False positives reported
- ❌ Scanner crashes

**What's Being Tested**: No false positives on clean code

---

### Step 5: Test CWE-78 Detection (Command Injection)

**Action**: Create a test manifest with command injection

```bash
mkdir -p /tmp/puppet-sec-test/modules/test/manifests

cat > /tmp/puppet-sec-test/modules/test/manifests/init.pp <<'EOF'
class test {
  exec { 'dangerous':
    command => "/bin/sh -c 'echo $input'",
  }
}
EOF

./target/release/rbuilder puppet security-scan /tmp/puppet-sec-test/modules/test
```

**Expected Output**:
```
[Critical] Potential command injection in exec resource...
  CWE: CWE-78
```

**Pass Criteria**:
- ✅ CWE-78 finding detected
- ✅ Severity is Critical
- ✅ Identifies exec resource

**Fail Criteria**:
- ❌ Not detected
- ❌ Wrong CWE

**What's Being Tested**: Command injection pattern detection

---

### Step 6: Test CWE-798 Detection (Hardcoded Secrets)

**Action**: Create manifest with hardcoded credentials

```bash
cat > /tmp/puppet-sec-test/modules/test/manifests/secret.pp <<'EOF'
class test::secret {
  file { '/etc/app/config':
    content => 'api_key=sk_live_hardcoded123',
  }
}
EOF

./target/release/rbuilder puppet security-scan /tmp/puppet-sec-test/modules/test
```

**Expected Output**:
```
[High] Potential hardcoded secret in resource...
  CWE: CWE-798
```

**Pass Criteria**:
- ✅ CWE-798 finding detected
- ✅ Detects "api_key" pattern
- ✅ Suggests using lookup()

**Fail Criteria**:
- ❌ Not detected
- ❌ Wrong severity

**What's Being Tested**: Hardcoded secret detection

---

### Step 7: Test CWE-732 Detection (File Permissions)

**Action**: Create manifest with insecure permissions

```bash
cat > /tmp/puppet-sec-test/modules/test/manifests/perms.pp <<'EOF'
class test::perms {
  file { '/tmp/data':
    mode => '0777',
  }
}
EOF

./target/release/rbuilder puppet security-scan /tmp/puppet-sec-test/modules/test
```

**Expected Output**:
```
[Medium] Insecure file permissions in file resource...
  CWE: CWE-732
```

**Pass Criteria**:
- ✅ CWE-732 finding detected
- ✅ Identifies 0777 as insecure
- ✅ Suggests restrictive modes

**Fail Criteria**:
- ❌ Not detected
- ❌ Accepts world-writable as safe

**What's Being Tested**: File permission validation

---

### Step 8: Test Graph-Based Scanning

**Action**: Scan from indexed graph instead of filesystem

```bash
# Initialize graph
./target/release/rbuilder init tests/fixtures/puppet

# Scan from graph
./target/release/rbuilder puppet security-scan tests/fixtures/puppet/modules/nginx \
  --from-graph
```

**Expected Output**:
```
[Critical] Potential command injection...
[High] Potential hardcoded secret...
[Medium] Insecure file permissions...
```

**Pass Criteria**:
- ✅ Same findings as filesystem scan
- ✅ Uses indexed graph nodes
- ✅ Faster than filesystem scan

**Fail Criteria**:
- ❌ Different findings
- ❌ Missing vulnerabilities

**What's Being Tested**: Graph-based security scanning

---

## Test Summary

### CWE Pattern Detection

| CWE | Pattern | Detected | Severity | Status |
|-----|---------|----------|----------|--------|
| **CWE-78** | Command injection (unquoted vars) | ⬜ | Critical | ⬜ |
| **CWE-798** | Hardcoded secrets | ⬜ | High | ⬜ |
| **CWE-732** | Insecure file permissions | ⬜ | Medium | ⬜ |

### Checklist

- [ ] Step 1: All 3 vulnerabilities detected in nginx
- [ ] Step 2: Severity filtering works correctly
- [ ] Step 3: JSON output is valid
- [ ] Step 4: No false positives on clean code
- [ ] Step 5: CWE-78 detection works
- [ ] Step 6: CWE-798 detection works
- [ ] Step 7: CWE-732 detection works
- [ ] Step 8: Graph-based scanning matches filesystem

### Result

**Overall Status**: ⬜ Not Run / ✅ Pass / ❌ Fail

**Detection Rate**: _____ / 3 CWE patterns detected

**False Positive Rate**: _____ %

**Notes**:
```
[Record any observations]
```

### Security Metrics

| Metric | Value | Target | Status |
|--------|-------|--------|--------|
| True Positives | | 3 | ⬜ |
| False Positives | | 0 | ⬜ |
| False Negatives | | 0 | ⬜ |
| Scan Time | | < 100ms | ⬜ |

### Issues Found

| Step | Issue | Severity |
|------|-------|----------|
| - | - | - |

### Next Steps

If all checks pass: ✅ **Proceed to Script 05 (Graph Queries)**

If any check fails:
1. Verify security patterns in scanner code
2. Check severity threshold logic
3. Review false positive cases
4. Re-run failed steps

---

**Test Executed By**: _______________  
**Date**: _______________  
**Signature**: _______________
