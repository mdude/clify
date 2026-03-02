# Clify

> **Clify makes your software cliable.**

Generate fully-featured CLI tools from API specifications. Define your API commands declaratively in YAML, and Clify generates a compiled Rust CLI binary with auth, output formatting, pagination, shell completions, and more.

## Quick Start

```bash
# Scan an OpenAPI spec
clify scan --from openapi ./api-spec.yaml

# Review and curate the generated spec
vim api.clify.yaml

# Generate the CLI project
clify generate api.clify.yaml --output ./my-cli

# Build
clify build --release
```

Or just run `clify` for the interactive TUI.

## Features (v0.1)

- 📄 **Declarative specs** — YAML-based, human-readable, auto-generatable
- 🔍 **API scanning** — auto-generate specs from OpenAPI 3.x and Swagger 2.0
- ⚙️ **Code generation** — produces compilable Rust CLI projects
- 🔐 **Auth** — API key, bearer token, basic, OAuth2 with custom hooks
- 📊 **Output** — JSON, table, CSV with pretty-printing
- 📄 **Pagination** — offset, cursor, link — auto-followed
- 🐚 **Shell completions** — bash, zsh, fish, PowerShell
- 🖥️ **Interactive TUI** — guided workflows via ratatui
- 🏗️ **Single binary** — generated CLIs compile to one static binary

## Spec Format

See [CLIFY-SPEC.md](docs/CLIFY-SPEC.md) for the full specification.

## Architecture

```
clify-core      Spec parsing, validation, code generation
clify-cli       The clify binary (TUI + commands)
clify-runtime   Shared runtime for generated CLIs (auth, HTTP, output)
```

## License

Apache-2.0
