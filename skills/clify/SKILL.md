---
name: clify
version: 0.1.0
description: "Generate CLI tools and agent skills from API specs. Clify makes software cliable."
metadata:
  openclaw:
    category: "developer-tools"
    requires:
      bins: ["clify"]
    optional:
      bins: ["cargo", "rustc"]
    cliHelp: "clify --help"
---

# Clify — Make Software Cliable

Clify generates fully-featured CLI binaries and AI agent skills from declarative YAML specs wrapping REST APIs. One spec file produces a compiled Rust CLI with auth, output formatting, pagination, and shell completions — plus a full set of agent skills so AI can use it too.

**You are helping the user create a CLI for their API.** This is a conversational, iterative process. Guide them through it step by step.

## Prerequisites

Clify itself is a single binary — no dependencies needed to run it.

**To compile generated CLIs**, the user needs Rust:

```bash
# Check
rustc --version && cargo --version

# Install if missing
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
```

## Core Workflow

The typical flow is: **scan → curate → validate → generate → build → skills**

Not every step is always needed. Adapt based on what the user has.

### 1. Starting from an OpenAPI/Swagger spec

If the user has an existing API spec (OpenAPI 3.x or Swagger 2.0):

```bash
# From a local file
clify scan --from openapi ./openapi.yaml -o my-api.clify.yaml

# From a URL
clify scan --from openapi https://petstore3.swagger.io/api/v3/openapi.json -o petstore.clify.yaml

# Swagger 2.0
clify scan --from swagger ./swagger.json -o my-api.clify.yaml
```

After scanning, **always validate** — scanned specs often need curation:

```bash
clify validate my-api.clify.yaml
```

Common post-scan fixes:
- Relative `base_url` → prepend the actual server URL
- Duplicate command names → rename or remove duplicates
- Missing auth → add the `auth` section manually
- Too many commands → remove ones users don't need

### 2. Starting from scratch

If there's no OpenAPI spec, scaffold a template:

```bash
clify init -o ./my-project
```

Then help the user fill in `api.clify.yaml` by asking about:
1. **What's the API?** → fill in `meta` (name, description)
2. **What's the base URL?** → fill in `transport`
3. **How does auth work?** → fill in `auth` (see [Auth Strategies](#auth-strategies))
4. **What are the key operations?** → create `commands` one by one

### 3. Validate

Always validate before generating:

```bash
clify validate my-api.clify.yaml
```

The validator catches 20+ structural issues with clear error messages. Fix what it reports, then re-validate.

### 4. Generate the CLI

```bash
clify generate my-api.clify.yaml --output ./generated
```

This produces a complete Rust project. The generated CLI includes:
- All commands from the spec with typed parameters
- `auth login / status / logout` — credential management
- `config set / get / list / reset` — persistent configuration
- `--output json|table|csv` on every command
- `--dry-run`, `--verbose`, `--base-url`, `--token` flags
- Shell completions (bash, zsh, fish, PowerShell)

### 5. Build

```bash
cd generated/my-api
cargo build --release
```

Or use Clify's build wrapper:

```bash
clify build --release
clify build --release --target aarch64-apple-darwin  # cross-compile
```

The binary is at `target/release/my-api`.

### 6. Generate Agent Skills

```bash
clify skills my-api.clify.yaml --output ./generated
```

This produces a `skills/` directory with:
- **Shared skill** — auth patterns, global flags, safety rules
- **Service skills** — per-group command reference
- **Action skills** — per-command detailed usage with params, examples, cautions

Options:
- `--no-actions` — only shared + service skills (fewer files)
- `--no-examples` — skip example generation
- `--category <cat>` — custom category in skill frontmatter

### 7. Export JSON Schema (optional)

For IDE autocomplete when editing `.clify.yaml` files:

```bash
clify schema -o clify-spec.schema.json
```

## Auth Strategies

Clify supports 5 auth types. Ask the user which matches their API:

| Type | When to use | Key fields |
|------|-------------|------------|
| `none` | Public APIs, no auth needed | — |
| `api-key` | API key in header or query param | `location`, `name`, `env` |
| `token` | Bearer token in Authorization header | `env` |
| `basic` | HTTP Basic Auth (username:password) | `env_user`, `env_pass` |
| `oauth2` | OAuth 2.0 (most enterprise APIs) | `grant`, `token_url`, `env_client_id`, `env_client_secret` |

OAuth2 supports custom token endpoints (e.g., ArcGIS `generateToken`):

```yaml
auth:
  type: oauth2
  grant: client_credentials
  token_url: "https://example.com/oauth/token"
  env_client_id: MY_CLIENT_ID
  env_client_secret: MY_CLIENT_SECRET
  custom:
    token_field: "access_token"
    expiry_field: "expires_in"
    content_type: form
    extra_params:
      f: "json"
```

## Spec Quick Reference

A `.clify.yaml` has these top-level sections:

```yaml
meta:           # name, version, description
transport:      # type: rest, base_url, timeout, retries, headers
auth:           # type: none | api-key | token | basic | oauth2
output:         # default_format, pretty, table style
config:         # config file path
groups:         # command groups (optional, for organization)
commands:       # the actual CLI commands
hooks:          # global before/after hooks (optional)
```

### Command structure:

```yaml
commands:
  - name: list-users           # kebab-case
    description: "List users"   # shown in --help
    group: users                # optional grouping
    request:
      method: GET
      path: "/users"
    params:
      - name: limit
        type: integer
        required: false
        description: "Max results"
        source: query           # where the param goes: path | query | body | header
        default: 25
    response:
      success_status: [200]
      success_path: "data.users"  # jq-style path to extract results
      pagination:
        type: offset            # offset | cursor | link
        param: offset
        page_size_param: limit
        default_page_size: 25
    examples:
      - description: "List first 10 users"
        command: "myapi users list-users --limit 10"
```

### Parameter types:

`string`, `integer`, `float`, `boolean`, `enum`, `array`, `file`, `object`

### Parameter sources:

- `path` — interpolated into the URL path (e.g., `/users/{id}`)
- `query` — URL query parameter
- `body` — included in request body
- `header` — sent as HTTP header

## Conversational Guidance

When helping a user build a CLI, follow this approach:

### If they have an OpenAPI spec:
1. Ask for the spec file/URL
2. Run `clify scan` to bootstrap the `.clify.yaml`
3. Validate and show any issues
4. Help curate: fix auth, remove unnecessary commands, improve descriptions
5. Generate, build, and verify with `--help` and `--dry-run`
6. Generate skills

### If they're starting from scratch:
1. Ask: "What API are you wrapping? What's the base URL?"
2. Ask: "How does authentication work?"
3. Ask: "What are the 3-5 most important operations?"
4. Write the `.clify.yaml` incrementally, validating as you go
5. Generate and test
6. Iterate — add more commands, refine params

### If they want to iterate:
- The spec is the source of truth — edit it, re-generate, re-build
- `clify validate` after every edit
- `clify generate` overwrites cleanly — safe to re-run

### General tips:
- **Start small.** 3-5 commands first, expand later.
- **Name commands with kebab-case verbs:** `list-users`, `create-project`, `get-status`
- **Group related commands** under groups for cleaner `--help` output
- **Always include `--dry-run` testing** before making real API calls
- **Use examples in the spec** — they become part of the generated `--help` and agent skills
- **Generate skills alongside the CLI** — the CLI makes it machine-callable, skills make it machine-understandable

## Troubleshooting

| Problem | Fix |
|---------|-----|
| `clify scan` fails to parse | Check if the spec is valid OpenAPI 3.x or Swagger 2.0 JSON/YAML |
| Validation errors after scan | Common — scanned specs need curation. Fix auth, base_url, duplicates |
| Generated CLI won't compile | Run `clify validate` first. Check Rust version (1.75+). |
| `cargo build` slow | Normal for first build (~2-3 min). Subsequent builds are fast (~5s). |
| Auth not working in generated CLI | Verify env vars are set. Use `my-cli auth status` to debug. |

## See Also

- [Full spec reference](https://github.com/mdude/clify/blob/main/docs/CLIFY-SPEC.md)
- [ArcGIS example spec](https://github.com/mdude/clify/blob/main/examples/example-arcgis-server.clify.yaml)
- [GitHub repo](https://github.com/mdude/clify)
