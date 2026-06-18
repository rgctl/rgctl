#!/bin/bash
# AI Agent Code Review Script for rBuilder
# Performs automated checks for code quality, consistency, and best practices

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "🤖 AI Agent Code Review for rBuilder"
echo "===================================="
echo ""

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

success_count=0
warning_count=0
error_count=0

# 1. Format Check
echo "📋 Checking code format..."
if cargo fmt -- --check > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Format check passed${NC}"
    ((success_count++))
else
    echo -e "${RED}❌ Format check failed. Run: cargo fmt${NC}"
    ((error_count++))
fi
echo ""

# 2. Clippy Check
echo "📋 Running clippy..."
if cargo clippy --all-targets --all-features -- -D warnings 2>&1 | tee /tmp/clippy_output.txt | tail -5; then
    echo -e "${GREEN}✅ Clippy passed${NC}"
    ((success_count++))
else
    echo -e "${RED}❌ Clippy found issues (see output above)${NC}"
    ((error_count++))
fi
echo ""

# 3. Build Check
echo "📋 Building project..."
if cargo build --all-features > /dev/null 2>&1; then
    echo -e "${GREEN}✅ Build successful${NC}"
    ((success_count++))
else
    echo -e "${RED}❌ Build failed${NC}"
    ((error_count++))
fi
echo ""

# 4. Test Check
echo "📋 Running tests..."
if cargo test --all-features 2>&1 | tail -10; then
    echo -e "${GREEN}✅ All tests passed${NC}"
    ((success_count++))
else
    echo -e "${RED}❌ Tests failed${NC}"
    ((error_count++))
fi
echo ""

# 5. Test Count Check
echo "📋 Checking test coverage per phase..."
for test_file in tests/phase{16,17,18}_*.rs; do
    if [ -f "$test_file" ]; then
        count=$(grep -cE '^fn test_' "$test_file" 2>/dev/null || echo "0")
        basename_file=$(basename "$test_file")
        echo "  $basename_file: $count tests"
        if [ $count -ge 30 ]; then
            echo -e "    ${GREEN}✅ Meets minimum (30+)${NC}"
            ((success_count++))
        elif [ $count -ge 25 ]; then
            echo -e "    ${YELLOW}⚠️  Close to target: $count/30${NC}"
            ((warning_count++))
        else
            echo -e "    ${RED}❌ Below minimum: $count/30${NC}"
            ((error_count++))
        fi
    fi
done
echo ""

# 6. Security Pattern Check
echo "📋 Checking security scanner CWE coverage..."
for scanner in src/security/{ansible,chef,puppet}.rs; do
    if [ -f "$scanner" ]; then
        cwe_count=$(grep -c '"CWE-' "$scanner" 2>/dev/null || echo "0")
        basename_scanner=$(basename "$scanner")
        echo "  $basename_scanner: $cwe_count CWE patterns"
        if [ $cwe_count -ge 3 ]; then
            echo -e "    ${GREEN}✅ Good CWE coverage${NC}"
            ((success_count++))
        else
            echo -e "    ${YELLOW}⚠️  Limited CWE coverage: $cwe_count${NC}"
            ((warning_count++))
        fi
    fi
done
echo ""

# 7. Documentation Check
echo "📋 Checking documentation build..."
doc_warnings=$(cargo doc --no-deps --all-features 2>&1 | grep -i "warning" | wc -l)
if [ $doc_warnings -gt 0 ]; then
    echo -e "  ${YELLOW}⚠️  Found $doc_warnings documentation warnings${NC}"
    ((warning_count++))
else
    echo -e "  ${GREEN}✅ Documentation builds cleanly${NC}"
    ((success_count++))
fi
echo ""

# 8. Unwrap Detection (dangerous patterns, excluding test modules)
echo "📋 Checking for dangerous patterns..."
unwrap_count=$(
    python3 - <<'PY'
import re, pathlib
unwrap = expect = panic = 0
for path in pathlib.Path("src").rglob("*.rs"):
    text = path.read_text()
    cleaned = re.sub(r"#\[cfg\(test\)\]\s*\nmod tests\s*\{.*?\n\}", "", text, flags=re.S)
    cleaned = re.sub(r"\nmod tests\s*\{.*?\n\}\n", "", cleaned, flags=re.S)
    for line in cleaned.splitlines():
        if ".unwrap()" in line:
            unwrap += 1
        if ".expect(" in line:
            expect += 1
        if "panic!" in line:
            panic += 1
print(unwrap, expect, panic)
PY
)
read -r unwrap_count expect_count panic_count <<< "$unwrap_count"

echo "  Found $unwrap_count unwrap() calls (excluding tests)"
echo "  Found $expect_count expect() calls (excluding tests)"
echo "  Found $panic_count panic!() calls (excluding tests)"

total_dangerous=$((unwrap_count + expect_count + panic_count))
if [ $total_dangerous -gt 100 ]; then
    echo -e "  ${RED}❌ High count of unwrap/expect/panic: $total_dangerous${NC}"
    echo "     Review error handling patterns"
    ((error_count++))
elif [ $total_dangerous -gt 50 ]; then
    echo -e "  ${YELLOW}⚠️  Moderate count of unwrap/expect/panic: $total_dangerous${NC}"
    ((warning_count++))
else
    echo -e "  ${GREEN}✅ Reasonable unwrap/expect/panic usage: $total_dangerous${NC}"
    ((success_count++))
fi
echo ""

# 9. Plugin Consistency Check
echo "📋 Checking multi-modal plugin consistency..."
for plugin_dir in src/languages/multimodal/{ansible,chef,puppet}; do
    if [ -d "$plugin_dir" ]; then
        plugin_name=$(basename "$plugin_dir")
        echo "  Checking $plugin_name plugin..."

        plugin_errors=0

        if [ -f "$plugin_dir/mod.rs" ]; then
            echo -e "    ${GREEN}✅ Has mod.rs${NC}"
        else
            echo -e "    ${RED}❌ Missing mod.rs${NC}"
            ((plugin_errors++))
        fi

        if [ -f "$plugin_dir/parser.rs" ]; then
            echo -e "    ${GREEN}✅ Has parser.rs${NC}"
        else
            echo -e "    ${RED}❌ Missing parser.rs${NC}"
            ((plugin_errors++))
        fi

        if [ -f "src/analysis/${plugin_name}_"*.rs ] || [ -f "src/analysis/${plugin_name}.rs" ]; then
            echo -e "    ${GREEN}✅ Has analysis module${NC}"
        else
            echo -e "    ${YELLOW}⚠️  No analysis module found${NC}"
            ((warning_count++))
        fi

        if [ -f "src/security/$plugin_name.rs" ]; then
            echo -e "    ${GREEN}✅ Has security scanner${NC}"
        else
            echo -e "    ${RED}❌ Missing security scanner${NC}"
            ((plugin_errors++))
        fi

        if [ -f "src/cli/$plugin_name.rs" ]; then
            echo -e "    ${GREEN}✅ Has CLI commands${NC}"
        else
            echo -e "    ${RED}❌ Missing CLI commands${NC}"
            ((plugin_errors++))
        fi

        test_file=$(find tests -name "phase*_$plugin_name.rs" 2>/dev/null | head -1)
        if [ -n "$test_file" ] && [ -f "$test_file" ]; then
            echo -e "    ${GREEN}✅ Has test file: $(basename "$test_file")${NC}"
        else
            echo -e "    ${RED}❌ Missing test file${NC}"
            ((plugin_errors++))
        fi

        if [ -f "docs/${plugin_name}_support.md" ]; then
            echo -e "    ${GREEN}✅ Has documentation${NC}"
        else
            echo -e "    ${RED}❌ Missing documentation${NC}"
            ((plugin_errors++))
        fi

        if [ $plugin_errors -eq 0 ]; then
            ((success_count++))
        else
            ((error_count++))
        fi
        echo ""
    fi
done

# 10. CLI Consistency Check
echo "📋 Checking CLI command consistency..."
for cli in src/cli/{ansible,chef,puppet}.rs; do
    if [ -f "$cli" ]; then
        cli_name=$(basename "$cli" .rs)
        echo "  Checking $cli_name CLI..."

        cli_issues=0

        grep -q "show_deps" "$cli" && echo -e "    ${GREEN}✅ Has --show-deps flag${NC}" || { echo -e "    ${YELLOW}⚠️  Missing --show-deps flag${NC}"; ((cli_issues++)); }
        grep -q "format" "$cli" && echo -e "    ${GREEN}✅ Has --format flag${NC}" || { echo -e "    ${YELLOW}⚠️  Missing --format flag${NC}"; ((cli_issues++)); }
        grep -q "from_graph" "$cli" && echo -e "    ${GREEN}✅ Has --from-graph flag${NC}" || { echo -e "    ${YELLOW}⚠️  Missing --from-graph flag${NC}"; ((cli_issues++)); }

        if grep -q "SecurityScan" "$cli"; then
            grep -q "min_severity" "$cli" && echo -e "    ${GREEN}✅ Has --min-severity flag${NC}" || { echo -e "    ${YELLOW}⚠️  Missing --min-severity flag${NC}"; ((cli_issues++)); }
        fi

        if [ $cli_issues -eq 0 ]; then
            ((success_count++))
        else
            ((warning_count++))
        fi
        echo ""
    fi
done

# 11. File Organization Check
echo "📋 Checking file organization..."
required_files=(
    "CODE_REVIEW_GUIDE.md"
    "AI_AGENT_REVIEW_GUIDE.md"
    "README.md"
    "Cargo.toml"
    ".github/TASK_PLAN.md"
)

for file in "${required_files[@]}"; do
    if [ -f "$file" ]; then
        echo -e "  ${GREEN}✅ $file exists${NC}"
    else
        echo -e "  ${RED}❌ $file missing${NC}"
        ((error_count++))
    fi
done
echo ""

# Summary
echo "════════════════════════════════════════"
echo "📊 Review Summary"
echo "════════════════════════════════════════"
echo ""
echo -e "${GREEN}✅ Successes: $success_count${NC}"
echo -e "${YELLOW}⚠️  Warnings:  $warning_count${NC}"
echo -e "${RED}❌ Errors:    $error_count${NC}"
echo ""

if [ $error_count -eq 0 ] && [ $warning_count -eq 0 ]; then
    echo -e "${GREEN}🎉 All checks passed! Code is ready for review.${NC}"
    exit 0
elif [ $error_count -eq 0 ]; then
    echo -e "${YELLOW}⚠️  Some warnings found. Review recommended.${NC}"
    exit 0
else
    echo -e "${RED}❌ Errors found. Please address issues before review.${NC}"
    exit 1
fi
