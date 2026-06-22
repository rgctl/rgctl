# Test Script 01: Setup and Verification

**Test ID**: UAT-PUPPET-01  
**Duration**: 5 minutes  
**Difficulty**: Easy  
**Prerequisites**: None

---

## Objective

Verify that rBuilder is installed with Puppet support and test environment is ready.

---

## Test Steps

### Step 1: Verify rBuilder Installation

**Action**: Check rBuilder version

```bash
cd ~/git/rust/rBuilder
./target/release/rbuilder --version
```

**Expected Output**:
```
rbuilder 0.1.0
```

**Pass Criteria**: ✅ Version number displays without error

**Fail Criteria**: ❌ Command not found, or version doesn't display

---

### Step 2: Check Puppet Feature Flag

**Action**: Verify Puppet support is compiled in

```bash
./target/release/rbuilder --help | grep -A 5 "puppet"
```

**Expected Output**:
```
  puppet     Puppet-specific commands (modules, security, validation)
```

**Pass Criteria**: ✅ "puppet" subcommand appears in help text

**Fail Criteria**: ❌ No "puppet" subcommand listed

**Troubleshooting**:
If puppet command is missing, rebuild with:
```bash
cargo build --release --features bundle-extended
```

---

### Step 3: Verify Test Data Exists

**Action**: Check that test data directory is present

```bash
ls -la user-acceptance-tests/test-data/puppet-sample/modules/
```

**Expected Output**:
```
drwxr-xr-x  webserver
drwxr-xr-x  database
drwxr-xr-x  common
```

**Pass Criteria**: ✅ All 3 module directories exist

**Fail Criteria**: ❌ Directory doesn't exist or is empty

---

### Step 4: Test Basic Puppet Command

**Action**: Run puppet help to see available commands

```bash
./target/release/rbuilder puppet --help
```

**Expected Output**:
```
Puppet-specific commands (modules, security, validation)

Usage: rbuilder puppet <COMMAND>

Commands:
  modules         Analyze Puppet modules and show dependencies
  validate        Validate Puppet manifests
  security-scan   Run security scan on Puppet modules
  help            Print this message or the help of the given subcommand(s)
```

**Pass Criteria**: ✅ All 3 subcommands listed (modules, validate, security-scan)

**Fail Criteria**: ❌ Command fails or subcommands missing

---

### Step 5: Check Test Fixtures

**Action**: Verify test fixtures from main test suite are present

```bash
ls -la tests/fixtures/puppet/modules/
```

**Expected Output**:
```
drwxr-xr-x  nginx
drwxr-xr-x  common
```

**Pass Criteria**: ✅ nginx and common modules exist

**Fail Criteria**: ❌ Fixtures missing

---

## Test Summary

### Checklist

- [ ] Step 1: rBuilder version displays correctly
- [ ] Step 2: Puppet subcommand is available
- [ ] Step 3: Test data directory exists
- [ ] Step 4: Puppet help shows all subcommands
- [ ] Step 5: Test fixtures are present

### Result

**Overall Status**: ⬜ Not Run / ✅ Pass / ❌ Fail

**Notes**:
```
[Record any observations or issues here]
```

### Issues Found

| Step | Issue | Severity |
|------|-------|----------|
| - | - | - |

### Next Steps

If all checks pass: ✅ **Proceed to Script 02 (Puppet Parsing)**

If any check fails:
1. Review the troubleshooting section
2. Rebuild rBuilder with correct features
3. Re-run this script

---

**Test Executed By**: _______________  
**Date**: _______________  
**Signature**: _______________
