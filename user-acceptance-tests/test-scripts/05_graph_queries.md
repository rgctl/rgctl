# Test Script 05: Graph Queries

**Test ID**: UAT-PUPPET-05  
**Duration**: 15 minutes  
**Difficulty**: Medium  
**Prerequisites**: Scripts 01 and 02 passed (graph initialized)

---

## Objective

Validate that Puppet nodes and edges are correctly indexed in the knowledge graph and can be queried using rBuilder's query language.

---

## Test Steps

### Step 1: Query All Puppet Modules

**Action**: Find all Puppet module nodes

```bash
cd ~/git/rust/rBuilder

./target/release/rbuilder query "type:PuppetModule"
```

**Expected Output**:
```
Found 2 results:

1. module::nginx (PuppetModule)
   Location: tests/fixtures/puppet/modules/nginx/metadata.json:1
   Metadata: {"version": "1.0.0", "module": "nginx"}

2. module::common (PuppetModule)
   Location: tests/fixtures/puppet/modules/common/metadata.json:1
   Metadata: {"version": "1.0.0", "module": "common"}
```

**Pass Criteria**:
- ✅ Both modules found
- ✅ Metadata includes version
- ✅ Correct node type (PuppetModule)

**Fail Criteria**:
- ❌ No results
- ❌ Missing metadata
- ❌ Wrong file locations

**What's Being Tested**: PuppetModule node indexing

---

### Step 2: Query Puppet Classes

**Action**: Find all class definitions

```bash
./target/release/rbuilder query "type:PuppetClass"
```

**Expected Output** (partial):
```
Found 3+ results:

1. class::nginx (PuppetClass)
   Location: tests/fixtures/puppet/modules/nginx/manifests/init.pp:2
   Metadata: {"class": "nginx"}

2. class::common (PuppetClass)
   Location: tests/fixtures/puppet/modules/common/manifests/init.pp:1
   Metadata: {"class": "common"}

3. class::nginx::base (PuppetClass)
   Metadata: {"referenced": true}
```

**Pass Criteria**:
- ✅ At least 3 classes found
- ✅ Each has class metadata
- ✅ File locations are correct

**Fail Criteria**:
- ❌ No classes found
- ❌ Missing metadata

**What's Being Tested**: PuppetClass node indexing

---

### Step 3: Query Puppet Resources

**Action**: Find all declared resources

```bash
./target/release/rbuilder query "type:PuppetResource"
```

**Expected Output** (partial):
```
Found 4+ results:

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
- ✅ At least 4 resources found
- ✅ Different resource types (package, service, file, exec)
- ✅ Metadata includes resource_type and title

**Fail Criteria**:
- ❌ No resources
- ❌ All same type
- ❌ Missing metadata

**What's Being Tested**: PuppetResource node indexing

---

### Step 4: Query with GQL - Find Module Dependencies

**Action**: Use GQL to find module dependency edges

```bash
./target/release/rbuilder gql "edges WHERE type = 'DependsOnModule'"
```

**Expected Output**:
```
Found 1 result:

Edge: module::nginx -> module::common (DependsOnModule)
  Location: tests/fixtures/puppet/modules/nginx/metadata.json:1
```

**Pass Criteria**:
- ✅ DependsOnModule edge found
- ✅ Direction is correct (nginx depends on common)
- ✅ Edge type is DependsOnModule

**Fail Criteria**:
- ❌ No edges found
- ❌ Wrong direction
- ❌ Wrong edge type

**What's Being Tested**: Module dependency edge indexing

---

### Step 5: Query for Class Relationships

**Action**: Find class inclusion relationships

```bash
./target/release/rbuilder gql "edges WHERE type = 'IncludesClass'"
```

**Expected Output**:
```
Found 1+ results:

Edge: class::nginx -> class::common (IncludesClass)
  Location: tests/fixtures/puppet/modules/nginx/manifests/init.pp:3
```

**Pass Criteria**:
- ✅ IncludesClass edge found
- ✅ nginx includes common
- ✅ Line number is accurate

**Fail Criteria**:
- ❌ No edges
- ❌ Wrong relationship

**What's Being Tested**: Class inclusion edge indexing

---

### Step 6: Query for Resource Declarations

**Action**: Find resources declared by classes

```bash
./target/release/rbuilder gql "edges WHERE type = 'DeclaresResource' AND from CONTAINS 'nginx'"
```

**Expected Output** (partial):
```
Found 4+ results:

Edge: class::nginx -> class::nginx::resource::package::nginx (DeclaresResource)
Edge: class::nginx -> class::nginx::resource::service::nginx (DeclaresResource)
Edge: class::nginx -> class::nginx::resource::file::/etc/nginx/nginx.conf (DeclaresResource)
Edge: class::nginx -> class::nginx::resource::exec::reload (DeclaresResource)
```

**Pass Criteria**:
- ✅ Multiple DeclaresResource edges
- ✅ All point from class to resources
- ✅ Correct resource IDs

**Fail Criteria**:
- ❌ No edges
- ❌ Wrong direction

**What's Being Tested**: Resource declaration edge indexing

---

### Step 7: Query for Resource Relationships

**Action**: Find resource-to-resource relationships (notify, require)

```bash
./target/release/rbuilder gql "edges WHERE type IN ['NotifiesResource', 'RequiresResource']"
```

**Expected Output**:
```
Found 2 results:

Edge: class::nginx::resource::package::nginx -> class::nginx::resource::service::nginx (NotifiesResource)
Edge: class::nginx::resource::service::nginx -> class::nginx::resource::package::nginx (RequiresResource)
```

**Pass Criteria**:
- ✅ NotifiesResource edge found (package notifies service)
- ✅ RequiresResource edge found (service requires package)
- ✅ Bidirectional relationship

**Fail Criteria**:
- ❌ No edges
- ❌ Missing one direction

**What's Being Tested**: Resource dependency edge indexing

---

### Step 8: Complex Query - Find Security-Critical Resources

**Action**: Query for exec resources (often security-sensitive)

```bash
./target/release/rbuilder query "type:PuppetResource" | grep "exec"
```

**Expected Output**:
```
class::nginx::resource::exec::reload (PuppetResource)
  Metadata: {"resource_type": "exec", "title": "reload"}
```

**Pass Criteria**:
- ✅ Exec resource found
- ✅ Can filter by resource_type in metadata

**Fail Criteria**:
- ❌ No exec resources
- ❌ Can't filter by type

**What's Being Tested**: Metadata-based filtering

---

### Step 9: Query Puppet Facts

**Action**: Find fact usage

```bash
./target/release/rbuilder query "type:PuppetFact"
```

**Expected Output**:
```
Found 1+ results:

1. fact::os (PuppetFact)
   Location: tests/fixtures/puppet/modules/nginx/manifests/init.pp:4
```

**Pass Criteria**:
- ✅ At least 1 fact found
- ✅ Fact name is correct (os)
- ✅ Line number where fact is used

**Fail Criteria**:
- ❌ No facts
- ❌ Wrong names

**What's Being Tested**: Fact usage tracking

---

### Step 10: Query Puppet Variables

**Action**: Find variable definitions

```bash
./target/release/rbuilder query "type:PuppetVariable"
```

**Expected Output**:
```
Found 1+ results:

1. var::nginx::web_port (PuppetVariable)
   Location: tests/fixtures/puppet/modules/nginx/manifests/init.pp:28
   Metadata: {"value": "$port"}
```

**Pass Criteria**:
- ✅ Variable found
- ✅ Value metadata present
- ✅ Scoped to class (var::nginx::)

**Fail Criteria**:
- ❌ No variables
- ❌ Missing value

**What's Being Tested**: Variable extraction and scoping

---

### Step 11: Performance Test

**Action**: Measure query performance

```bash
# Query with timing
time ./target/release/rbuilder query "type:PuppetClass" > /dev/null
```

**Expected Output**:
```
real    0m0.030s
user    0m0.020s
sys     0m0.008s
```

**Pass Criteria**:
- ✅ Query completes in < 100ms
- ✅ No errors

**Fail Criteria**:
- ❌ Takes > 1 second
- ❌ Query hangs

**What's Being Tested**: Query performance

---

## Test Summary

### Node Type Coverage

| Node Type | Expected Count | Actual Count | Status |
|-----------|----------------|--------------|--------|
| PuppetModule | 2 | | ⬜ |
| PuppetClass | 3+ | | ⬜ |
| PuppetResource | 4+ | | ⬜ |
| PuppetFact | 1+ | | ⬜ |
| PuppetVariable | 1+ | | ⬜ |

### Edge Type Coverage

| Edge Type | Expected Count | Actual Count | Status |
|-----------|----------------|--------------|--------|
| DependsOnModule | 1+ | | ⬜ |
| IncludesClass | 1+ | | ⬜ |
| DeclaresResource | 4+ | | ⬜ |
| NotifiesResource | 1+ | | ⬜ |
| RequiresResource | 1+ | | ⬜ |

### Checklist

- [ ] Step 1: Module nodes queryable
- [ ] Step 2: Class nodes queryable
- [ ] Step 3: Resource nodes queryable
- [ ] Step 4: Module dependency edges found
- [ ] Step 5: Class inclusion edges found
- [ ] Step 6: Resource declaration edges found
- [ ] Step 7: Resource relationship edges found
- [ ] Step 8: Metadata filtering works
- [ ] Step 9: Fact nodes queryable
- [ ] Step 10: Variable nodes queryable
- [ ] Step 11: Query performance acceptable

### Result

**Overall Status**: ⬜ Not Run / ✅ Pass / ❌ Fail

**Coverage Score**: _____ / 11 steps passed

**Notes**:
```
[Record any observations]
```

### Performance Metrics

| Query Type | Time | Target | Status |
|------------|------|--------|--------|
| Simple type query | | < 50ms | ⬜ |
| Complex GQL query | | < 100ms | ⬜ |
| Edge traversal | | < 100ms | ⬜ |

### Issues Found

| Step | Issue | Severity |
|------|-------|----------|
| - | - | - |

### Next Steps

If all checks pass: ✅ **Proceed to Script 06 (CLI Commands)**

If any check fails:
1. Verify graph initialization succeeded
2. Check node/edge indexing in graph backend
3. Review query syntax
4. Re-run failed steps

---

**Test Executed By**: _______________  
**Date**: _______________  
**Signature**: _______________
