# Test Script 02: Puppet Parsing

**Test ID**: UAT-PUPPET-02  
**Duration**: 10 minutes  
**Difficulty**: Easy  
**Prerequisites**: Script 01 passed

---

## Objective

Validate that rBuilder correctly parses Puppet manifests and extracts symbols, relations, and metadata.

---

## Test Steps

### Step 1: Parse a Simple Manifest

**Action**: Parse the nginx init.pp manifest

```bash
cd ~/git/rust/rBuilder
./target/release/rbuilder puppet validate tests/fixtures/puppet/modules/nginx/manifests/init.pp
```

**Expected Output**:
```
Valid Puppet manifest: 1 class symbol(s)
```

**Pass Criteria**: ✅ Output shows "Valid Puppet manifest" with class count

**Fail Criteria**: ❌ Parse error or "No Puppet class content found"

**What's Being Tested**: Basic manifest parsing with class extraction

---

### Step 2: Validate Module Directory

**Action**: Validate all manifests in a module

```bash
./target/release/rbuilder puppet validate tests/fixtures/puppet/modules/nginx/
```

**Expected Output**:
```
tests/fixtures/puppet/modules/nginx/manifests/init.pp: 5 symbol(s)
tests/fixtures/puppet/modules/nginx/manifests/server.pp: 3 symbol(s)
```

**Pass Criteria**: ✅ Both manifest files are listed with symbol counts > 0

**Fail Criteria**: ❌ No output or parse errors

**What's Being Tested**: Directory traversal and multi-file parsing

---

### Step 3: Check Symbol Extraction Details

**Action**: Initialize graph and query for extracted symbols

```bash
# Initialize graph from test fixtures
./target/release/rbuilder init tests/fixtures/puppet

# Query for Puppet classes
./target/release/rbuilder query "type:PuppetClass"
```

**Expected Output**:
```
Found 3 results:

1. class::nginx (PuppetClass)
   Location: tests/fixtures/puppet/modules/nginx/manifests/init.pp:2
   
2. class::nginx::base (PuppetClass)
   Location: tests/fixtures/puppet/modules/nginx/manifests/init.pp:2
   
3. class::common (PuppetClass)
   Location: tests/fixtures/puppet/modules/common/manifests/init.pp:1
```

**Pass Criteria**: 
- ✅ At least 3 PuppetClass nodes found
- ✅ Each has a valid file location
- ✅ Names follow "class::" prefix pattern

**Fail Criteria**: 
- ❌ Zero results
- ❌ Missing file locations
- ❌ Incorrect node types

**What's Being Tested**: Symbol extraction and graph indexing

---

### Step 4: Verify Resource Extraction

**Action**: Query for Puppet resources

```bash
./target/release/rbuilder query "type:PuppetResource"
```

**Expected Output**:
```
Found 4 results:

1. class::nginx::resource::package::nginx (PuppetResource)
   Metadata: {"resource_type": "package", "title": "nginx"}

2. class::nginx::resource::service::nginx (PuppetResource)
   Metadata: {"resource_type": "service", "title": "nginx"}

3. class::nginx::resource::file::/etc/nginx/nginx.conf (PuppetResource)
   Metadata: {"resource_type": "file", "title": "/etc/nginx/nginx.conf"}

4. class::nginx::resource::exec::reload (PuppetResource)
   Metadata: {"resource_type": "exec", "title": "reload"}
```

**Pass Criteria**:
- ✅ Multiple PuppetResource nodes found
- ✅ Each has resource_type metadata
- ✅ Resource types include: package, service, file, exec

**Fail Criteria**:
- ❌ Zero resources found
- ❌ Missing metadata
- ❌ Wrong resource types

**What's Being Tested**: Resource declaration extraction

---

### Step 5: Check Metadata Parsing

**Action**: Validate metadata.json parsing

```bash
# The metadata.json should already be indexed, query for modules
./target/release/rbuilder query "type:PuppetModule"
```

**Expected Output**:
```
Found 2 results:

1. module::nginx (PuppetModule)
   Metadata: {"version": "1.0.0", "module": "nginx"}
   
2. module::common (PuppetModule)
   Metadata: {"version": "1.0.0", "module": "common"}
```

**Pass Criteria**:
- ✅ Module nodes found
- ✅ Version metadata present
- ✅ Module names correct

**Fail Criteria**:
- ❌ No modules found
- ❌ Missing version
- ❌ Incorrect names

**What's Being Tested**: JSON metadata parsing

---

### Step 6: Verify Relationships

**Action**: Check that relations are extracted (e.g., class includes, resource dependencies)

```bash
# Look for "include common" relation
./target/release/rbuilder gql "edges WHERE from = 'class::nginx'"
```

**Expected Output** (partial):
```
Edge: class::nginx -> class::common (IncludesClass)
Edge: class::nginx -> class::nginx::resource::package::nginx (DeclaresResource)
Edge: class::nginx -> class::nginx::resource::service::nginx (DeclaresResource)
```

**Pass Criteria**:
- ✅ IncludesClass edge found (nginx includes common)
- ✅ DeclaresResource edges found (class declares resources)
- ✅ Edge types are correct

**Fail Criteria**:
- ❌ No edges found
- ❌ Wrong edge types
- ❌ Missing expected relationships

**What's Being Tested**: Relation extraction (includes, declares, inherits)

---

### Step 7: Test Variable and Fact Extraction

**Action**: Query for variables and facts

```bash
# Variables
./target/release/rbuilder query "type:PuppetVariable"

# Facts
./target/release/rbuilder query "type:PuppetFact"
```

**Expected Output**:
```
# Variables:
Found 1 result:
1. var::nginx::web_port (PuppetVariable)
   Metadata: {"value": "$port"}

# Facts:
Found 1 result:
1. fact::os (PuppetFact)
```

**Pass Criteria**:
- ✅ At least 1 variable found
- ✅ At least 1 fact found
- ✅ Variable has value metadata

**Fail Criteria**:
- ❌ No variables or facts found
- ❌ Missing metadata

**What's Being Tested**: Variable assignment and fact usage extraction

---

### Step 8: Test Error Handling

**Action**: Try to parse an invalid manifest

```bash
# Create a malformed file
echo "this is not valid puppet code {}{}{" > /tmp/bad.pp

# Try to validate it
./target/release/rbuilder puppet validate /tmp/bad.pp 2>&1
```

**Expected Output**:
```
Error: No Puppet class content found
```

**Pass Criteria**: 
- ✅ Command returns error (non-zero exit code)
- ✅ Error message is clear
- ✅ No panic or crash

**Fail Criteria**:
- ❌ Command crashes
- ❌ Unclear error message
- ❌ Process hangs

**What's Being Tested**: Graceful error handling

---

## Test Summary

### Parsing Accuracy

| Feature | Expected Count | Actual Count | Status |
|---------|----------------|--------------|--------|
| PuppetClass | 3+ | | ⬜ |
| PuppetResource | 4+ | | ⬜ |
| PuppetModule | 2 | | ⬜ |
| PuppetVariable | 1+ | | ⬜ |
| PuppetFact | 1+ | | ⬜ |

### Checklist

- [ ] Step 1: Simple manifest parses correctly
- [ ] Step 2: Directory validation works
- [ ] Step 3: Classes extracted with locations
- [ ] Step 4: Resources extracted with metadata
- [ ] Step 5: Metadata.json parsed correctly
- [ ] Step 6: Relationships extracted
- [ ] Step 7: Variables and facts found
- [ ] Step 8: Error handling is graceful

### Result

**Overall Status**: ⬜ Not Run / ✅ Pass / ❌ Fail

**Accuracy Score**: _____ / 8 steps passed

**Notes**:
```
[Record any observations]
```

### Performance Metrics

| Operation | Time | Target | Status |
|-----------|------|--------|--------|
| Parse single manifest | | < 50ms | ⬜ |
| Validate directory | | < 500ms | ⬜ |
| Build graph | | < 2s | ⬜ |

### Issues Found

| Step | Issue | Severity |
|------|-------|----------|
| - | - | - |

### Next Steps

If all checks pass: ✅ **Proceed to Script 03 (Module Dependencies)**

If any check fails:
1. Review parse errors
2. Check test fixture files exist
3. Verify graph initialization succeeded
4. Re-run failed steps

---

**Test Executed By**: _______________  
**Date**: _______________  
**Signature**: _______________
