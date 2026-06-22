# Test Data for Puppet UAT

This directory contains sample Puppet modules for user acceptance testing.

## Module Structure

```
puppet-sample/
└── modules/
    ├── webserver/       # Web server with security issues (intentional)
    ├── database/        # PostgreSQL database module
    └── common/          # Base utilities (clean code)
```

## Module Relationships

```
webserver → common
webserver → firewall (external)
database  → common
```

**Topological order**: `common`, `firewall`, `database`, `webserver`

## Security Issues (Intentional)

The `webserver` module contains intentional vulnerabilities for testing:

1. **CWE-78** - Command injection in `restart-webserver` exec resource
   - Unquoted variable `$server_name` in shell command
   
2. **CWE-798** - Hardcoded secret in `/etc/webserver/config.conf`
   - Password hardcoded as `SuperSecret123!`
   
3. **CWE-732** - Insecure file permissions
   - Config file is world-writable (mode `0777`)

## Module Details

### webserver (v2.1.0)
- **Purpose**: HTTP server setup with security testing
- **Dependencies**: common, puppetlabs-firewall
- **Files**: 
  - `metadata.json` - Module metadata with dependencies
  - `manifests/init.pp` - Main webserver class
- **Security**: Contains 3 CWE patterns
- **Resources**: exec, file, package, service, notify
- **Features**: Fact usage, resource relationships

### database (v1.5.2)
- **Purpose**: PostgreSQL database installation
- **Dependencies**: common
- **Files**:
  - `metadata.json` - Module metadata
  - `manifests/init.pp` - Database class with inheritance
- **Security**: Clean (no issues)
- **Resources**: package, service, file, cron
- **Features**: Class inheritance, Hiera lookups, conditional logic

### common (v1.2.0)
- **Purpose**: Base utilities and configuration
- **Dependencies**: None (leaf node)
- **Files**:
  - `metadata.json` - Module metadata
  - `manifests/init.pp` - Common utilities
- **Security**: Clean (no issues)
- **Resources**: package, file, service
- **Features**: Simple, well-formed code

## Expected Test Results

### Parsing
- **Total classes**: 4 (webserver, database, database::base, common)
- **Total resources**: ~15 across all modules
- **Total variables**: 5+
- **Total facts**: 2+ (hostname, os)

### Dependencies
- **Module count**: 3
- **Dependency edges**: 2 (webserver→common, database→common)
- **External dependencies**: 1 (webserver→firewall)

### Security
- **Total findings**: 3 (all in webserver module)
- **Critical**: 1 (CWE-78)
- **High**: 1 (CWE-798)
- **Medium**: 1 (CWE-732)

### Graph Nodes
- **PuppetModule**: 3
- **PuppetClass**: 4
- **PuppetResource**: 15+
- **PuppetVariable**: 5+
- **PuppetFact**: 2+

### Graph Edges
- **DependsOnModule**: 2
- **IncludesClass**: 2
- **DeclaresResource**: 15+
- **NotifiesResource**: 2+
- **RequiresResource**: 3+

## Usage in Tests

### Parse All Modules
```bash
rbuilder puppet validate user-acceptance-tests/test-data/puppet-sample/modules/
```

### Analyze Dependencies
```bash
rbuilder puppet modules user-acceptance-tests/test-data/puppet-sample/modules/ --show-deps
```

### Security Scan
```bash
rbuilder puppet security-scan user-acceptance-tests/test-data/puppet-sample/modules/webserver
```

### Build Graph
```bash
rbuilder init user-acceptance-tests/test-data/puppet-sample
rbuilder query "type:PuppetModule"
```

## Notes

- All modules use Puppet 6+ syntax
- Test data is self-contained (no external dependencies except puppetlabs-firewall reference)
- Security issues are clearly marked in comments
- Clean modules (database, common) can be used to test for false positives
