# Natural Language Query Examples

rBuilder supports natural language queries powered by LLMs, making the knowledge graph accessible to everyone.

## Basic Usage

```bash
# One-shot query
rbuilder ask "How many React components am I using?"

# Interactive conversation mode
rbuilder chat

# Show the translated graph query
rbuilder ask "Find all services" --explain
```

---

## Example Queries by Category

### 1. Inventory & Discovery

**Q: "How many React components am I using?"**
```
Answer:
You have 156 React components across your codebase:
- 82 in the 'ui' community (src/components/)
- 45 in the 'dashboard' community (src/pages/)
- 29 in the 'shared' community (src/common/)

Top 5 most connected components:
1. AppLayout - called by 34 other components
2. DataTable - called by 28 other components
3. Modal - called by 22 other components
4. Button - called by 45 other components
5. Form - called by 18 other components
```

**Q: "Give me all the SOA services you have"**
```
Answer:
Found 12 SOA services in your codebase:

1. AuthenticationService (src/services/auth.rs)
   - 8 methods
   - Complexity: MEDIUM (avg 8)
   - Dependencies: TokenService, UserRepository
   
2. UserManagementService (src/services/user.py)
   - 15 methods
   - Complexity: HIGH (avg 14)
   - Dependencies: EmailService, AuditService
   
3. PaymentService (src/services/payment.go)
   - 22 methods
   - Complexity: CRITICAL (avg 18)
   - Dependencies: StripeAPI, TransactionRepository
   
[... 9 more ...]
```

**Q: "List all deprecated APIs"**
```
Answer:
Found 8 deprecated functions:

🔴 CRITICAL (still heavily used):
1. legacy_authentication() - src/auth/legacy.rs:45
   - Deprecated since: v2.0
   - Called by: 23 functions
   - Replacement: use authenticate_v3()
   
2. xml_parser_v1() - src/parsers/xml.py:120
   - Deprecated since: v1.5
   - Called by: 12 functions
   - Replacement: use xml_parser_v2()

⚠️ MODERATE (some usage):
[... 4 more ...]

✅ LOW (minimal usage):
[... 2 more ...]

💡 Migration priority: Focus on legacy_authentication() first (23 callers)
```

---

### 2. Code Quality & Complexity

**Q: "What are my most complex functions?"**
```
Answer:
Top 10 Most Complex Functions:

1. 🔴 process_payment_with_retry() - CRITICAL
   📍 src/payment/processor.rs:245
   📊 Cyclomatic: 45, Cognitive: 52
   🏷️ Labels: security:critical, payment:core
   🔗 Called by 8 functions
   💡 Suggestion: Consider splitting into smaller functions

2. 🔴 authenticate_with_mfa() - CRITICAL
   📍 src/auth/service.rs:120
   📊 Cyclomatic: 38, Cognitive: 42
   🏷️ Labels: security:critical, auth:core
   🔗 Called by 5 functions

[... 8 more ...]

📈 Complexity Distribution:
   CRITICAL (21+): 15 functions (3%)
   HIGH (11-20):   78 functions (15%)
   MEDIUM (6-10):  234 functions (45%)
   LOW (0-5):      192 functions (37%)
```

**Q: "Find all high-complexity security functions"**
```
Answer:
Found 8 security-critical functions with high complexity:

🔴 CRITICAL PRIORITY:
1. verify_jwt_signature() - Complexity: 28
   - Location: src/auth/jwt.rs:89
   - Security: Handles token validation
   - Recommendation: Add comprehensive unit tests
   
2. encrypt_sensitive_data() - Complexity: 24
   - Location: src/crypto/encryption.rs:145
   - Security: Encryption of PII
   - Recommendation: Security audit required

[... 6 more ...]

🚨 ACTION REQUIRED:
   - 8 functions need immediate security review
   - 6 functions lack sufficient test coverage
   - Consider refactoring to reduce complexity
```

**Q: "Which functions have no tests?"**
```
Answer:
Found 67 functions without tests:

🔴 HIGH PRIORITY (complex + no tests):
1. calculate_tax() - Complexity: 18, No tests
2. validate_payment() - Complexity: 15, No tests
3. process_refund() - Complexity: 22, No tests

⚠️ MEDIUM PRIORITY (moderate complexity):
[... 24 more ...]

✅ LOW PRIORITY (simple functions):
[... 40 more ...]

📊 Overall test coverage: 73% (387/519 functions)
```

---

### 3. Impact Analysis

**Q: "What if I wanted to change verify_token(), what other things will be affected?"**
```
Answer:
🔍 Analyzing impact of changing verify_token()...

⚠️ HIGH IMPACT - This change affects 23 functions across 3 communities

🔴 DIRECT DEPENDENCIES (6 functions directly call it):
1. authenticate_user() - src/auth/middleware.rs:120
   - Complexity: HIGH (18)
   - Labels: security:critical
   - Called by 15 endpoints
   
2. refresh_session() - src/auth/session.rs:67
   - Complexity: MEDIUM (12)
   - Called by 8 functions
   
[... 4 more ...]

⚠️ INDIRECT DEPENDENCIES (17 functions call the direct dependents):
   - 12 API endpoints (authentication would fail)
   - 3 background jobs (session cleanup would fail)
   - 2 admin tools

📊 Community Impact:
   🔴 'auth' community: 15/67 functions affected (22%)
   ⚠️ 'api' community: 5/120 functions affected (4%)
   ✅ 'admin' community: 3/45 functions affected (7%)

🧪 Test Coverage:
   ✅ verify_token() has 95% coverage
   ⚠️ Only 34% of dependents have integration tests

💡 Migration Path:
   1. Add integration tests for all 15 affected endpoints
   2. Update authenticate_user() first (highest impact)
   3. Gradually update remaining 5 direct callers
   4. Monitor authentication metrics during rollout

🚨 RECOMMENDATION: High-risk change. Consider:
   - Feature flag for gradual rollout
   - Extended monitoring period
   - Rollback plan
```

**Q: "What would break if I delete UserRepository?"**
```
Answer:
🔍 Analyzing impact of deleting UserRepository...

🚨 CRITICAL IMPACT - This change affects 47 functions across 4 communities

🔴 DIRECT DEPENDENCIES (12 functions):
   - UserService (8 methods)
   - AuthService (2 methods)
   - AdminController (2 methods)

⚠️ INDIRECT DEPENDENCIES (35 functions):
   - 18 API endpoints (user operations would fail)
   - 12 background jobs (user sync would fail)
   - 5 scheduled tasks

📊 Blast Radius:
   - 47 functions (9% of codebase)
   - 4 communities
   - 18 API endpoints
   - 12 background jobs

💡 This is a FOUNDATIONAL component. Deletion requires major refactoring.
```

---

### 4. Configuration Analysis

**Q: "Which configuration keys are never used?"**
```
Answer:
Found 14 unused configuration keys:

🔴 PRODUCTION CONFIGS (likely dangerous):
1. 'legacy.feature_flag' in config/production.yaml
   - Value: true
   - Last modified: 6 months ago
   - Recommendation: Verify before removal

2. 'cache.redis.backup_host' in config/app.yaml
   - Value: redis-backup.internal
   - No code references found
   - Recommendation: May be used by infrastructure

⚠️ DEVELOPMENT CONFIGS:
3. 'debug.verbose_logging' in .env.example
   - Example file only
   - Safe to remove

[... 11 more ...]

💡 Total savings: ~15% reduction in config file size
```

**Q: "Find missing environment variables"**
```
Answer:
Found 7 missing environment variables:

🔴 CRITICAL (app won't start):
1. DATABASE_URL
   - Referenced in: src/database/connection.rs:45
   - Required for: Database connection
   - Example: postgres://user:pass@localhost/db

2. JWT_SECRET
   - Referenced in: src/auth/jwt.rs:89
   - Required for: Token signing
   - Example: (generate 32-byte random string)

⚠️ OPTIONAL (features degraded):
3. REDIS_URL
   - Referenced in: src/cache/redis.rs:23
   - Required for: Caching (falls back to in-memory)
   
[... 4 more ...]

📝 Create .env file with these variables to fix errors.
```

**Q: "Find hardcoded secrets in configuration files"**
```
Answer:
🚨 SECURITY ALERT: Found 5 potential secrets in configuration files

🔴 HIGH SEVERITY:
1. API Key in config/services.yaml:23
   - Key: 'stripe.api_key'
   - Value: 'sk_live_...' (hardcoded production key!)
   - 🚨 IMMEDIATE ACTION: Move to environment variable

2. Database password in config/database.yaml:12
   - Key: 'database.password'
   - Value: 'mysecretpassword123'
   - 🚨 IMMEDIATE ACTION: Use environment variable

⚠️ MEDIUM SEVERITY:
3. Email SMTP password in config/email.yaml:15
   - Should be in environment variable

[... 2 more ...]

💡 RECOMMENDATIONS:
   - Move all 5 secrets to environment variables
   - Add secrets scanning to CI/CD pipeline
   - Rotate exposed credentials immediately
```

---

### 5. Architecture & Design

**Q: "Show me the most connected modules"**
```
Answer:
Top 10 Most Connected Modules (by total connections):

1. 📦 auth (67 functions)
   - Incoming: 156 calls from other modules
   - Outgoing: 45 calls to other modules
   - Total degree: 201
   - Communities connected: 8
   - Key dependencies: database, cache, email

2. 📦 api (120 functions)
   - Incoming: 89 calls
   - Outgoing: 178 calls
   - Total degree: 267
   - Communities connected: 12
   
[... 8 more ...]

💡 High connectivity can indicate:
   - Core architectural components (good)
   - God modules that do too much (bad)
   - Opportunities for decoupling
```

**Q: "What are my architectural communities?"**
```
Answer:
Detected 8 architectural communities via Leiden algorithm:

1. 🏛️ 'auth' community (67 functions)
   - Purpose: Authentication and authorization
   - Modularity: 0.82 (well-isolated)
   - Key files: src/auth/, src/middleware/
   - External deps: database, cache
   
2. 🌐 'api' community (120 functions)
   - Purpose: REST API endpoints
   - Modularity: 0.74
   - Key files: src/api/, src/controllers/
   - External deps: auth, database, services

3. 🎨 'ui' community (156 functions)
   - Purpose: React UI components
   - Modularity: 0.89 (highly isolated)
   - Key files: src/components/, src/pages/
   - External deps: api (via HTTP)

[... 5 more ...]

📊 Overall modularity score: 0.78 (good architectural separation)
💡 Consider extracting 'auth' as a microservice (well-isolated, clear boundary)
```

**Q: "Find all circular dependencies"**
```
Answer:
Found 3 circular dependency cycles:

🔴 CYCLE 1 (4 modules):
   UserService → AuthService → SessionService → UserService
   - Cycle length: 3
   - Affects: 23 functions
   - Recommendation: Extract shared types/interfaces

⚠️ CYCLE 2 (2 modules):
   PaymentService ⇄ OrderService
   - Cycle length: 2
   - Affects: 12 functions
   - Recommendation: Introduce PaymentGateway abstraction

✅ CYCLE 3 (2 modules):
   ConfigLoader ⇄ Logger (acceptable - initialization cycle)
   - Cycle length: 2
   - Low risk: initialization code only

💡 Focus on breaking CYCLE 1 first (highest impact)
```

---

### 6. Conversational Queries

```bash
$ rbuilder chat

rBuilder> How many services do I have?
Found 12 SOA services in your codebase.

rBuilder> Which ones are in the auth module?
3 services in the 'auth' community:
1. AuthenticationService
2. AuthorizationService
3. TokenManagementService

rBuilder> What's the complexity of AuthenticationService?
AuthenticationService has:
- Cyclomatic complexity: 45 (CRITICAL)
- Cognitive complexity: 38 (CRITICAL)
- 8 methods, average complexity: 12 (HIGH)
- Most complex method: authenticate_with_mfa() (complexity: 22)

rBuilder> Show me that method
Function: authenticate_with_mfa()
Location: src/auth/service.rs:245
Complexity: Cyclomatic 22, Cognitive 28 (CRITICAL)
Called by: 5 functions
Calls: 12 external dependencies
Labels: security:critical, auth:core

Code preview:
```rust
pub async fn authenticate_with_mfa(
    user_id: Uuid,
    password: &str,
    mfa_code: &str
) -> Result<Session> {
    // ... 45 lines of complex logic ...
}
```

rBuilder> What calls it?
5 functions call authenticate_with_mfa():
1. login_handler() - src/api/auth_controller.rs:45
2. refresh_token_handler() - src/api/auth_controller.rs:120
3. admin_impersonate() - src/admin/actions.rs:200
4. cli_login() - src/cli/auth.rs:80
5. test_mfa_flow() - tests/auth_test.rs:150

rBuilder> What would break if I change it?
[... impact analysis as shown above ...]

rBuilder> exit
Goodbye!
```

---

## Advanced Queries

### Database & Performance

**Q: "Find all database queries that aren't using connection pooling"**
```
Answer:
Found 7 functions making direct database calls without pooling:

1. get_user_by_id() - src/user/repository.rs:45
   - Creates new connection each time
   - Recommendation: Inject ConnectionPool

2. save_transaction() - src/payment/db.rs:120
   - Opens connection in loop (performance issue!)
   
[... 5 more ...]
```

**Q: "Which functions do the most database calls?"**
```
Answer:
Top 5 database-heavy functions:

1. sync_all_users() - 23 database calls
   - Location: src/jobs/user_sync.rs:89
   - Recommendation: Use batch queries

2. generate_report() - 18 database calls
   - Location: src/reports/generator.rs:120
   - Recommendation: Consider caching
   
[... 3 more ...]
```

### Cross-Language Analysis

**Q: "How does my Python code interact with my Rust code?"**
```
Answer:
Found 12 cross-language interaction points:

🔗 FFI Calls (Rust ← Python):
1. rust_crypto.encrypt() called from python/crypto.py:45
2. rust_crypto.decrypt() called from python/crypto.py:67
3. rust_parser.parse_json() called from python/parsers/json.py:23

🌐 HTTP APIs (Python → Rust):
4. POST /api/auth/login (Python client → Rust server)
5. GET /api/users/{id} (Python admin → Rust API)

[... 7 more ...]
```

---

## Query Translation Examples (--explain flag)

```bash
$ rbuilder ask "How many React components?" --explain

Natural Language: "How many React components?"

Translated to Cypher:
MATCH (n:Function)
WHERE 'react:component' IN n.labels
   OR n.name =~ '.*Component$'
   OR (n)-[:Returns]->(t:Type {name: 'JSX.Element'})
RETURN COUNT(n) as total

Result: 156 components

---

$ rbuilder ask "What breaks if I change verify_token?" --explain

Natural Language: "What breaks if I change verify_token?"

Translated to Cypher:
MATCH (target:Function {name: 'verify_token'})
MATCH path = (caller)-[:Calls*1..3]->(target)
RETURN DISTINCT 
  caller.name, 
  caller.file_path,
  LENGTH(path) as depth,
  caller.complexity,
  caller.labels
ORDER BY depth ASC, caller.complexity DESC

Results: 23 affected functions
```

---

## Offline Mode (No LLM Required)

When no LLM is available, rBuilder falls back to template-based queries:

```bash
$ rbuilder ask "count components"
# Matches template: "count {label}"
# Executes: MATCH (n) WHERE '{label}' IN n.labels RETURN COUNT(n)

$ rbuilder ask "complexity of MyFunction"
# Matches template: "complexity of {function}"
# Executes: MATCH (n:Function {name: '{function}'}) RETURN n.complexity

$ rbuilder ask "what calls MyFunction"
# Matches template: "what calls {function}"
# Executes: MATCH (caller)-[:Calls]->(n:Function {name: '{function}'}) RETURN caller
```

Common templates:
- "count {label}"
- "list {label}"
- "complexity of {function}"
- "what calls {function}"
- "who uses {config_key}"
- "find unused {type}"

---

## Best Practices

1. **Be specific**: "Find all React components" is better than "find components"
2. **Use domain terms**: The system learns your project's terminology automatically
3. **Follow-up questions**: Use conversational mode for multi-step exploration
4. **Verify critical changes**: Always use `--explain` for impact analysis queries
5. **Combine with rules**: Use NLP to find issues, then create rules to prevent them

---

## Supported Question Patterns

✅ Inventory: "How many X?", "List all Y", "Give me all Z"
✅ Search: "Find X", "Show me Y", "Where is Z?"
✅ Quality: "What's the complexity of X?", "Find high-complexity Y"
✅ Impact: "What breaks if I change X?", "What uses Y?", "Dependencies of Z"
✅ Config: "Which config keys are unused?", "Find missing env vars"
✅ Architecture: "Show communities", "Find circular dependencies"
✅ Comparison: "What's the most connected module?", "Top 10 complex functions"

❌ Code generation: "Write a function to do X" (use /graphify skill instead)
❌ External knowledge: "What is OAuth?" (rBuilder only knows your codebase)
