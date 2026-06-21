# Remove Sensitive File Extensions from Knowledge Defaults

## Objective

Remove file extensions that may contain sensitive data (credentials, secrets,
internal configs) from the default `supportedKnowledgeExts` map in the
knowledge indexer.

## Changes

### `internal/knowledge/indexer.go`

Removed from default extensions:

| Extension | Reason |
|-----------|--------|
| `.yaml` / `.yml` | Config files often contain secrets, API keys, passwords |
| `.json` | Can contain credentials, tokens, internal configs |
| `.wsdl` | SOAP descriptors may expose internal endpoints and auth |
| `.xml` | General XML can contain sensitive structured data |

Kept in defaults:

| Extension | Reason |
|-----------|--------|
| `.proto` | Pure type/service definitions, no runtime data |
| `.graphql` / `.gql` | Schema-only files, no credentials expected |

## Notes

- Users can re-enable any removed extension via `knowledge.extensions` in
  project config (comma-separated, e.g., `md,yaml,json`).
- Default went from 16 → 11 extensions.
