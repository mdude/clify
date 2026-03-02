# Clify Spec Format v0.1

> *The `.clify.yaml` file is the single source of truth for generating a CLI binary.*

---

## Table of Contents

- [Overview](#overview)
- [File Structure](#file-structure)
- [Sections](#sections)
  - [meta](#meta)
  - [transport](#transport)
  - [auth](#auth)
  - [output](#output)
  - [config](#config)
  - [groups](#groups)
  - [commands](#commands)
  - [hooks](#hooks)
- [Parameter Types](#parameter-types)
- [Parameter Sources](#parameter-sources)
- [Response Handling](#response-handling)
- [Pagination](#pagination)
- [Auth Strategies](#auth-strategies)
- [Validation Rules](#validation-rules)
- [Path Interpolation](#path-interpolation)
- [Generated CLI Behavior](#generated-cli-behavior)
- [Examples](#examples)

---

## Overview

A `.clify.yaml` file describes everything Clify needs to generate a fully-featured CLI binary:

- What API to talk to (transport)
- How to authenticate (auth)
- What commands are available, their parameters, and expected responses (commands)
- How to format output (output)

**Principles:**

1. **Declarative** — describe *what*, not *how*. No code in the spec.
2. **Complete** — the spec contains everything needed to generate a working CLI.
3. **Curateable** — `clify scan` auto-generates the spec from an API; teams then curate it.
4. **Extensible** — hooks allow pre/post processing without touching generated code.

**Workflow:**

```
OpenAPI/Swagger/URL  ──→  clify scan  ──→  .clify.yaml  ──→  clify generate  ──→  Rust CLI project
                                              ↑
                                         team curates
```

---

## File Structure

```yaml
meta:        # Project metadata (required)
transport:   # Backend connection config (required)
auth:        # Authentication config (required, can be type: none)
output:      # Output formatting defaults (optional)
config:      # User config storage (optional)
groups:      # Command group definitions (optional)
commands:    # Command definitions (required, at least one)
hooks:       # Global pre/post hooks (optional)
```

---

## Sections

### meta

Project-level metadata. Controls the generated binary name, version, and documentation.

| Field              | Type   | Required | Default | Description                              |
|--------------------|--------|----------|---------|------------------------------------------|
| `name`             | string | ✅       |         | CLI binary name. Must be a valid command name (lowercase, hyphens OK). Example: `arcgis-server` |
| `version`          | string | ✅       |         | Semantic version. Example: `0.1.0`       |
| `description`      | string | ✅       |         | One-line description shown in `--help` and shell completions |
| `long_description` | string |          |         | Multi-line description for man pages     |
| `author`           | string |          |         | Author name or organization              |
| `license`          | string |          |         | SPDX license identifier (e.g., `MIT`, `Apache-2.0`) |
| `homepage`         | string |          |         | Project URL                              |

**Example:**

```yaml
meta:
  name: arcgis-server
  version: "0.1.0"
  description: "CLI for ArcGIS Server geoprocessing and map services"
  author: "Esri"
  license: "Apache-2.0"
  homepage: "https://developers.arcgis.com"
```

**Constraints:**

- `name` must match regex `^[a-z][a-z0-9-]*$` (lowercase, starts with letter, hyphens allowed)
- `version` must be valid semver

---

### transport

Defines how the generated CLI communicates with the backend service.

| Field      | Type              | Required | Default            | Description                           |
|------------|-------------------|----------|--------------------|---------------------------------------|
| `type`     | enum              | ✅       |                    | Transport type. v0.1 supports: `rest` |
| `base_url` | string           | ✅       |                    | Default base URL. Users can override via config. Must include scheme (`https://`) |
| `timeout`  | integer           |          | `30`               | Default HTTP timeout in seconds       |
| `retries`  | integer           |          | `0`                | Number of retries on transient failure (5xx, timeout) |
| `headers`  | map<string,string>|          |                    | Default headers sent with every request |

**Example:**

```yaml
transport:
  type: rest
  base_url: "https://gis.example.com/arcgis/rest/services"
  timeout: 60
  retries: 2
  headers:
    Accept: "application/json"
    User-Agent: "arcgis-server-cli/0.1.0"
```

**Retry behavior:**

- Only retries on 5xx status codes and network timeouts
- Uses exponential backoff: 1s, 2s, 4s, ...
- Does not retry on 4xx (client errors)

**Future transport types (post v0.1):**

- `process` — wrap a local executable
- `library` — call into a language SDK (e.g., Python/ArcPy)
- `grpc` — gRPC services
- `graphql` — GraphQL APIs

---

### auth

Authentication configuration. Every generated CLI gets built-in `auth login`, `auth status`, and `auth logout` commands automatically.

| Field              | Type   | Required | Default | Description                          |
|--------------------|--------|----------|---------|--------------------------------------|
| `type`             | enum   | ✅       |         | Auth strategy (see below)            |

**Auth types:** `none`, `api-key`, `token`, `basic`, `oauth2`

#### type: none

No authentication. The generated CLI won't have `auth` commands.

```yaml
auth:
  type: none
```

#### type: api-key

A static API key sent as a header or query parameter.

| Field      | Type   | Required | Default  | Description                                  |
|------------|--------|----------|----------|----------------------------------------------|
| `location` | enum   | ✅       |          | Where to send the key: `header` or `query`   |
| `name`     | string | ✅       |          | Header name or query param name               |
| `env`      | string | ✅       |          | Environment variable to read the key from     |

```yaml
auth:
  type: api-key
  location: header
  name: "X-API-Key"
  env: ARCGIS_API_KEY
```

**Generated behavior:**

```bash
# Set via env
export ARCGIS_API_KEY=abc123
arcgis-server services list

# Set via login (stores in config)
arcgis-server auth login
# Prompts: API Key: ****

# Set per-request
arcgis-server services list --api-key abc123
```

#### type: token

A bearer token sent in the `Authorization: Bearer <token>` header.

| Field | Type   | Required | Description                          |
|-------|--------|----------|--------------------------------------|
| `env` | string | ✅       | Environment variable for the token   |

```yaml
auth:
  type: token
  env: ARCGIS_TOKEN
```

#### type: basic

HTTP Basic authentication (username + password → Base64 encoded).

| Field      | Type   | Required | Description                          |
|------------|--------|----------|--------------------------------------|
| `env_user` | string | ✅       | Env var for username                 |
| `env_pass` | string | ✅       | Env var for password                 |

```yaml
auth:
  type: basic
  env_user: ARCGIS_USER
  env_pass: ARCGIS_PASS
```

#### type: oauth2

OAuth 2.0 authentication with automatic token management.

| Field               | Type     | Required | Default          | Description                              |
|---------------------|----------|----------|------------------|------------------------------------------|
| `grant`             | enum     | ✅       |                  | Grant type: `client_credentials`, `authorization_code`, `device_code` |
| `token_url`         | string   | ✅       |                  | Token endpoint URL                       |
| `authorize_url`     | string   |          |                  | Authorization URL (for `authorization_code` grant) |
| `scopes`            | [string] |          | `[]`             | OAuth scopes to request                  |
| `env_client_id`     | string   | ✅       |                  | Env var for client ID                    |
| `env_client_secret` | string   | ✅       |                  | Env var for client secret                |
| `custom`            | object   |          |                  | Custom overrides for non-standard APIs   |

**Custom overrides** (for APIs that don't follow standard OAuth2):

| Field          | Type              | Default          | Description                               |
|----------------|-------------------|------------------|-------------------------------------------|
| `token_field`  | string            | `access_token`   | JSON field name containing the token      |
| `expiry_field` | string            | `expires_in`     | JSON field name for expiry (seconds)      |
| `content_type` | enum              | `json`           | Token request encoding: `json` or `form`  |
| `extra_params` | map<string,string>|                  | Additional params sent with token request |

```yaml
auth:
  type: oauth2
  grant: client_credentials
  token_url: "https://www.arcgis.com/sharing/rest/generateToken"
  env_client_id: ARCGIS_CLIENT_ID
  env_client_secret: ARCGIS_CLIENT_SECRET
  custom:
    token_field: "token"
    expiry_field: "expires"
    content_type: form
    extra_params:
      f: "json"
      referer: "https://www.arcgis.com"
```

**Generated behavior:**

- `auth login` performs the OAuth flow and stores the token
- Token is auto-refreshed when expired (before each request)
- Token stored in `~/.config/<name>/auth.json` (configurable)

#### Credential storage

| Field              | Type    | Default                            | Description                       |
|--------------------|---------|------------------------------------|-----------------------------------|
| `storage.path`     | string  | `~/.config/<meta.name>/auth.json`  | Where to store credentials        |
| `storage.encrypt`  | boolean | `false`                            | Encrypt stored credentials        |

#### Token resolution order

All auth types follow the same precedence:

1. **Explicit flag** — `--token`, `--api-key`, `--user`/`--pass` (highest priority)
2. **Environment variable** — as defined in the spec
3. **Stored credentials** — from `auth login`
4. **Interactive prompt** — if running in a TTY with no other source

---

### output

Default output formatting. Every generated command gets `--output` / `-o` and `--no-pretty` flags automatically.

| Field            | Type    | Required | Default  | Description                              |
|------------------|---------|----------|----------|------------------------------------------|
| `default_format` | enum    |          | `json`   | Default output format: `json`, `table`, `csv` |
| `pretty`         | boolean |          | `true`   | Pretty-print JSON by default             |
| `table.max_width`| integer |          | terminal | Max table width in columns               |
| `table.style`    | enum    |          | `plain`  | Table border style: `plain`, `rounded`, `sharp`, `markdown` |

```yaml
output:
  default_format: json
  pretty: true
  table:
    style: rounded
```

**Generated behavior:**

```bash
# Default: JSON
arcgis-server data query --service Roads --layer 0

# Table output
arcgis-server data query --service Roads --layer 0 -o table

# CSV (pipeable)
arcgis-server data query --service Roads --layer 0 -o csv > roads.csv

# Compact JSON (for piping to jq)
arcgis-server data query --service Roads --layer 0 --no-pretty | jq '.features[0]'
```

---

### config

Persistent user configuration. Generated CLI gets `config set`, `config get`, `config list`, `config reset` commands.

| Field  | Type   | Required | Default                              | Description            |
|--------|--------|----------|--------------------------------------|------------------------|
| `path` | string |          | `~/.config/<meta.name>/config.toml`  | Config file location   |

**Configurable keys (auto-generated):**

| Key             | Source             | Description                     |
|-----------------|--------------------|---------------------------------|
| `base_url`      | `transport.base_url` | Override the default base URL |
| `output_format` | `output.default_format` | Default output format      |
| `timeout`       | `transport.timeout`  | Request timeout               |
| `pretty`        | `output.pretty`      | Pretty-print JSON             |

```bash
# Set a different server
arcgis-server config set base_url https://production.example.com/arcgis/rest/services

# View current config
arcgis-server config list

# Reset to defaults
arcgis-server config reset
```

---

### groups

Optional command grouping for organization. Groups become subcommand namespaces.

| Field         | Type   | Required | Description                          |
|---------------|--------|----------|--------------------------------------|
| `name`        | string | ✅       | Group name (used in command line)    |
| `description` | string | ✅       | Shown in `--help`                    |

```yaml
groups:
  - name: analysis
    description: "Spatial analysis and geoprocessing tools"
  - name: data
    description: "Data management and feature services"
```

**Result:**

```
$ arcgis-server --help

Usage: arcgis-server <COMMAND>

Commands:
  analysis   Spatial analysis and geoprocessing tools
  data       Data management and feature services
  auth       Authentication management
  config     Configuration management
  help       Print help

$ arcgis-server analysis --help

Commands:
  buffer     Create buffer zones around features
  overlay    Overlay two feature layers
```

**Rules:**

- Group names must match regex `^[a-z][a-z0-9-]*$`
- Commands without a `group` field become top-level subcommands
- `auth`, `config`, and `help` are reserved group names (auto-generated)

---

### commands

The core of the spec. Each entry becomes a CLI subcommand.

| Field              | Type     | Required | Default | Description                              |
|--------------------|----------|----------|---------|------------------------------------------|
| `name`             | string   | ✅       |         | Command name                             |
| `description`      | string   | ✅       |         | Short description for `--help`           |
| `long_description` | string   |          |         | Detailed help text                       |
| `group`            | string   |          |         | Parent group name (must exist in `groups`) |
| `aliases`          | [string] |          | `[]`    | Alternative names (e.g., `["buf"]`)      |
| `hidden`           | boolean  |          | `false` | Hide from `--help` (still callable)      |
| `request`          | object   | ✅       |         | HTTP request configuration               |
| `params`           | [object] |          | `[]`    | Parameter definitions                    |
| `response`         | object   |          |         | Response handling configuration          |
| `examples`         | [object] |          | `[]`    | Usage examples shown in `--help`         |
| `hooks`            | object   |          |         | Per-command hooks                        |

#### command.request

| Field          | Type              | Required | Default | Description                          |
|----------------|-------------------|----------|---------|--------------------------------------|
| `method`       | enum              | ✅       |         | HTTP method: `GET`, `POST`, `PUT`, `PATCH`, `DELETE` |
| `path`         | string            | ✅       |         | URL path (appended to `transport.base_url`). Supports `{param}` interpolation. |
| `content_type` | enum              |          | `json`  | Request body encoding: `json`, `form`, `multipart` |
| `headers`      | map<string,string>|          |         | Per-command headers (merged with transport defaults) |

#### command.params

Each param becomes a CLI flag.

| Field         | Type     | Required | Default | Description                              |
|---------------|----------|----------|---------|------------------------------------------|
| `name`        | string   | ✅       |         | Parameter name. Becomes `--name` flag. Use hyphens for multi-word (e.g., `spatial-rel` → `--spatial-rel`). |
| `type`        | enum     | ✅       |         | Data type (see [Parameter Types](#parameter-types)) |
| `required`    | boolean  |          | `false` | Is this parameter mandatory?             |
| `description` | string   | ✅       |         | Help text for `--help`                   |
| `short`       | string   |          |         | Single-char short flag (e.g., `"d"` → `-d`) |
| `default`     | any      |          |         | Default value when not provided          |
| `env`         | string   |          |         | Environment variable override            |
| `source`      | enum     |          | auto    | Where to put the param in the HTTP request (see [Parameter Sources](#parameter-sources)) |
| `hidden`      | boolean  |          | `false` | Hide from `--help`                       |
| `values`      | [string] |          |         | Allowed values (for `type: enum`)        |
| `separator`   | string   |          | `","`   | Delimiter (for `type: array`)            |
| `file_type`   | enum     |          | `path`  | File input mode (for `type: file`): `path`, `stdin`, `both` |
| `mime_type`   | string   |          |         | Expected MIME type (for `type: file`)    |
| `validation`  | object   |          |         | Validation rules (see [Validation Rules](#validation-rules)) |

#### command.response

| Field            | Type      | Required | Default   | Description                          |
|------------------|-----------|----------|-----------|--------------------------------------|
| `success_status` | [integer] |          | `[200]`   | HTTP status codes indicating success |
| `success_path`   | string    |          |           | jq-style path to extract result data |
| `error_path`     | string    |          |           | jq-style path to extract error message |
| `pagination`     | object    |          |           | Pagination config (see [Pagination](#pagination)) |

#### command.examples

| Field         | Type   | Required | Description                          |
|---------------|--------|----------|--------------------------------------|
| `description` | string | ✅       | What this example demonstrates       |
| `command`     | string | ✅       | Full command line                    |

Examples appear in `--help` output:

```
EXAMPLES:
    # Buffer roads by 100 meters
    arcgis-server analysis buffer -i roads.geojson -d 100 -u meters

    # Buffer with dissolve
    arcgis-server analysis buffer -i parcels.geojson -d 500 --dissolve dissolve
```

#### command.hooks

| Field      | Type   | Description                                      |
|------------|--------|--------------------------------------------------|
| `before`   | string | Script/command to run before the HTTP request     |
| `after`    | string | Script/command to run after successful response   |
| `on_error` | string | Script/command to run on error                    |

Hook scripts receive context via environment variables:

| Variable             | Description                          |
|----------------------|--------------------------------------|
| `CLIFY_COMMAND`      | Command name                         |
| `CLIFY_METHOD`       | HTTP method                          |
| `CLIFY_URL`          | Full request URL                     |
| `CLIFY_PARAMS_JSON`  | All params as JSON                   |
| `CLIFY_RESPONSE_BODY`| Response body (after hooks only)     |
| `CLIFY_STATUS_CODE`  | HTTP status code (after hooks only)  |

---

### hooks

Global hooks that run for every command.

| Field           | Type   | Description                          |
|-----------------|--------|--------------------------------------|
| `global.before` | string | Runs before every command            |
| `global.after`  | string | Runs after every command             |

Per-command hooks take precedence over global hooks. Both can coexist.

---

## Parameter Types

| Type      | CLI Input                  | Sent As             | Notes                                |
|-----------|----------------------------|---------------------|--------------------------------------|
| `string`  | `--name value`             | String              | Default type                         |
| `integer` | `--count 10`               | Number              | Validated as integer                 |
| `float`   | `--distance 3.14`          | Number              | Validated as float                   |
| `boolean` | `--verbose` (flag)         | `true`/`false`      | Presence = true, absence = false (or default) |
| `enum`    | `--type MapServer`         | String              | Validated against `values` list      |
| `array`   | `--ids 1,2,3`              | Array or CSV string | Split by `separator` (default: `,`) |
| `file`    | `--input data.json`        | File contents       | Read from path (or stdin if `file_type: stdin`) |
| `object`  | `--geometry '{"x":1,"y":2}'` | Object            | Parsed as JSON                       |

### File type details

When `type: file`, the generated CLI:

1. Reads the file from the given path
2. For `content_type: multipart` → sends as multipart form upload
3. For `content_type: json` → embeds file contents in the JSON body
4. For `content_type: form` → sends file contents as form field value

If `file_type: stdin`, accepts piped input: `cat data.json | arcgis-server analysis buffer -i -`

If `file_type: both`, accepts either a path or `-` for stdin.

---

## Parameter Sources

The `source` field controls where in the HTTP request a parameter is placed.

| Source   | Behavior                                            |
|----------|-----------------------------------------------------|
| `path`   | Interpolated into the URL path via `{param_name}`   |
| `query`  | Added as a query string parameter                   |
| `body`   | Included in the request body (JSON, form, or multipart) |
| `header` | Sent as an HTTP header                              |

**Auto-detection** (when `source` is not specified):

- `GET`, `DELETE` → defaults to `query`
- `POST`, `PUT`, `PATCH` → defaults to `body`
- If param name matches a `{placeholder}` in `request.path` → `path` (regardless of method)

---

## Response Handling

### Success path extraction

`success_path` uses jq-style dot notation to extract the relevant data from the response:

```yaml
response:
  success_path: "results[0].value"
```

| Expression          | Extracts                                |
|---------------------|-----------------------------------------|
| `results`           | `response.results`                      |
| `results[0]`        | First element of `response.results`     |
| `data.features`     | `response.data.features`                |
| (empty/omitted)     | Entire response body                    |

### Error handling

`error_path` extracts the error message for display:

```yaml
response:
  error_path: "error.message"
```

The generated CLI checks:

1. HTTP status code against `success_status` (default: `[200]`)
2. If status matches but response contains an error at `error_path` → still report as error (common in APIs like ArcGIS that return 200 with error bodies)
3. Display extracted error message or fall back to raw status code + body

---

## Pagination

For APIs that return paginated results, Clify auto-follows pages.

| Field               | Type    | Required | Description                              |
|---------------------|---------|----------|------------------------------------------|
| `type`              | enum    | ✅       | `offset`, `cursor`, or `link`            |
| `param`             | string  | ✅       | Query param name for the page token/offset |
| `page_size_param`   | string  |          | Query param for page size                |
| `default_page_size` | integer |          | Default items per page                   |
| `next_path`         | string  |          | jq path to next cursor (for `cursor` type) |
| `total_path`        | string  |          | jq path to total count (for `offset` type) |

### Pagination types

**offset** — uses numeric offset:

```yaml
pagination:
  type: offset
  param: "resultOffset"
  page_size_param: "resultRecordCount"
  default_page_size: 1000
  total_path: "count"
```

Generated request flow: `?resultOffset=0&resultRecordCount=1000` → `?resultOffset=1000&resultRecordCount=1000` → ...

**cursor** — uses opaque token:

```yaml
pagination:
  type: cursor
  param: "cursor"
  page_size_param: "limit"
  default_page_size: 100
  next_path: "next_cursor"
```

**link** — follows RFC 5988 `Link` header:

```yaml
pagination:
  type: link
```

### User control

Generated CLI adds pagination flags to paginated commands:

```bash
# Get all results (auto-paginate)
arcgis-server data query --service Roads --layer 0

# Limit to first page
arcgis-server data query --service Roads --layer 0 --no-paginate

# Custom page size
arcgis-server data query --service Roads --layer 0 --page-size 500

# Limit total results
arcgis-server data query --service Roads --layer 0 --max-results 5000
```

---

## Path Interpolation

URL paths support `{param_name}` placeholders that are filled from parameters with `source: path`.

**Spec:**

```yaml
request:
  path: "/{service}/FeatureServer/{layer}/query"

params:
  - name: service
    type: string
    source: path
  - name: layer
    type: integer
    source: path
    default: 0
```

**Usage:**

```bash
arcgis-server data query --service Transportation --layer 0 --where "TYPE='highway'"
```

**Resulting URL:**

```
https://gis.example.com/arcgis/rest/services/Transportation/FeatureServer/0/query?where=TYPE%3D%27highway%27
```

**Rules:**

- Every `{placeholder}` in the path MUST have a corresponding param with `source: path`
- Path params are always required (even if `required: false` is set) unless they have a `default`
- Values are URL-encoded automatically

---

## Validation Rules

Optional validation constraints on parameters.

| Field        | Type    | Applies To        | Description                          |
|--------------|---------|-------------------|--------------------------------------|
| `min`        | number  | integer, float    | Minimum value (inclusive)            |
| `max`        | number  | integer, float    | Maximum value (inclusive)            |
| `min_length` | integer | string, array     | Minimum length/count                 |
| `max_length` | integer | string, array     | Maximum length/count                 |
| `pattern`    | string  | string            | Regex pattern the value must match   |
| `custom`     | string  | any               | Path to a validation script          |

```yaml
params:
  - name: distance
    type: float
    validation:
      min: 0
      max: 100000

  - name: email
    type: string
    validation:
      pattern: "^[\\w.-]+@[\\w.-]+\\.[a-zA-Z]{2,}$"

  - name: name
    type: string
    validation:
      min_length: 1
      max_length: 255
```

**Validation happens client-side** before making the HTTP request. Errors are shown immediately:

```
Error: --distance must be >= 0 (got: -5)
```

---

## Generated CLI Behavior

Every generated CLI includes these features automatically (not configured in spec):

### Built-in commands

| Command          | Description                          |
|------------------|--------------------------------------|
| `auth login`     | Interactive authentication           |
| `auth status`    | Show current auth state              |
| `auth logout`    | Clear stored credentials             |
| `config set`     | Set a config value                   |
| `config get`     | Get a config value                   |
| `config list`    | Show all config                      |
| `config reset`   | Reset to defaults                    |
| `help`           | Help for any command                 |

### Global flags

| Flag            | Short | Description                          |
|-----------------|-------|--------------------------------------|
| `--output`      | `-o`  | Output format: `json`, `table`, `csv` |
| `--no-pretty`   |       | Disable pretty-printing              |
| `--dry-run`     |       | Show HTTP request without executing  |
| `--verbose`     | `-v`  | Show request/response details        |
| `--quiet`       | `-q`  | Suppress non-essential output        |
| `--no-paginate` |       | Don't auto-follow pagination         |
| `--page-size`   |       | Override default page size           |
| `--max-results` |       | Limit total results                  |
| `--timeout`     |       | Override request timeout (seconds)   |
| `--base-url`    |       | Override base URL for this request   |
| `--token`       |       | Override auth token for this request |
| `--version`     | `-V`  | Show version                         |
| `--help`        | `-h`  | Show help                            |

### Exit codes

| Code | Meaning                              |
|------|--------------------------------------|
| `0`  | Success                              |
| `1`  | General error                        |
| `2`  | Invalid arguments / validation error |
| `3`  | Authentication error                 |
| `4`  | HTTP error (4xx)                     |
| `5`  | Server error (5xx)                   |
| `6`  | Network / timeout error              |
| `7`  | Hook script failed                   |

### Dry run output

```bash
$ arcgis-server analysis buffer -i roads.geojson -d 100 --dry-run

POST https://gis.example.com/arcgis/rest/services/Analysis/GPServer/Buffer/execute
Authorization: Bearer eyJ...
Content-Type: application/x-www-form-urlencoded

input=<contents of roads.geojson>&distance=100&units=meters&f=json
```

### Shell completions

Generated at build time for bash, zsh, fish, and PowerShell. Installed via:

```bash
# bash
arcgis-server completions bash >> ~/.bashrc

# zsh
arcgis-server completions zsh > ~/.zfunc/_arcgis-server

# fish
arcgis-server completions fish > ~/.config/fish/completions/arcgis-server.fish
```

Completions include command names, flag names, and enum values.

---

## Examples

### Minimal spec

```yaml
meta:
  name: my-api
  version: "0.1.0"
  description: "CLI for My API"

transport:
  type: rest
  base_url: "https://api.example.com/v1"

auth:
  type: api-key
  location: header
  name: "Authorization"
  env: MY_API_KEY

commands:
  - name: list-users
    description: "List all users"
    request:
      method: GET
      path: "/users"
    response:
      success_path: "data"
```

### Full spec

See [`example-arcgis-server.clify.yaml`](./example-arcgis-server.clify.yaml) for a comprehensive real-world example.

---

## JSON Schema

The `.clify.yaml` format is validated by `clify-spec.schema.json` (shipped with Clify). This enables:

- IDE autocomplete (VS Code, IntelliJ with YAML plugin)
- CI validation: `clify validate my-api.clify.yaml`
- Clear error messages on spec issues

Add to your `.clify.yaml` for IDE support:

```yaml
# yaml-language-server: $schema=https://clify.dev/schema/v0.1/clify-spec.schema.json
```
