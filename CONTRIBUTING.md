# Contributing to rBuilder

Thank you for your interest in contributing to rBuilder!

## Development Setup

### Prerequisites

- Rust 1.70+ (install from [rustup.rs](https://rustup.rs))
- Git

### Getting Started

1. **Clone the repository**:
   ```bash
   git clone https://github.com/yourusername/rbuilder
   cd rbuilder
   ```

2. **Build the project**:
   ```bash
   cargo build
   ```

3. **Run tests**:
   ```bash
   cargo test
   ```

4. **Run benchmarks**:
   ```bash
   cargo bench
   ```

## Project Structure

```
rBuilder/
├── src/
│   ├── extraction/      # AST parsing and symbol extraction
│   ├── languages/       # Language plugin system
│   ├── graph/           # Graph storage and query
│   ├── analysis/        # Graph analysis algorithms
│   ├── nlp/             # Natural language query processing
│   ├── mcp/             # MCP server for AI agents
│   ├── config/          # Configuration analysis
│   ├── rules/           # Rule engine
│   ├── semantic/        # IDL generation
│   └── ...
├── benches/             # Performance benchmarks
├── tests/               # Integration tests
└── examples/            # Example usage