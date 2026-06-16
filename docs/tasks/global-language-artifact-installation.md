# Global Language Artifact Installation

## Summary

Changed language artifact installation from the Hub to target **global directories** (`~/.graphit/`) instead of **per-project directories** (`.graphit/`). Language definitions are universal and shared across all projects, making per-project installation unnecessary.

## Changes

### `internal/hub/grammar_install.go`

- `installGrammarArchive`: When `dotDir` is empty, grammar binaries are placed at `<baseDir>/grammars/{treesitter,antlr}/` instead of `<baseDir>/<dotDir>/grammars/...`. This supports the global install path where `baseDir = ~/.graphit/` and `dotDir = ""`.
- `uninstallGrammarFiles`: Same empty-dotDir handling for removal.

### `internal/hub/service.go`

- **Install** (`case TypeLanguage`): YAMLs go to `~/.graphit/ast/queries/`, grammar archives extract to `~/.graphit/grammars/`. Uses `brand.GlobalDir()` instead of `pp.ActiveProjectDir + brand.DotDir()`.
- **preUninstallHook** (`case TypeLanguage`): No-op — file cleanup is deferred to orphan check.
- **Uninstall** (`orphaned` check): When `RegisterUninstall` confirms no other project uses the artifact, `cleanupGlobalLanguageFiles` removes YAMLs and grammar binaries from global dirs.
- **Link from source** (`case TypeLanguage`): Links/copies language files to global dirs.

### `internal/hub/language_global.go` [NEW]

- `EnsureGlobalLanguageArtifacts(lockfilePath)`: For each hub-installed `TypeLanguage` artifact in the lockfile, ensures YAMLs and grammars are present in global dirs. Called during sync.

### `cmd/graphit/commands/lifecycle.go`

- `runSyncHeavyTasks`: After hub reconciliation, calls `EnsureGlobalLanguageArtifacts` to ensure global language files are up to date.

## Behavior

| Scenario | Before | After |
|----------|--------|-------|
| Hub install language | Files in `<project>/.graphit/` | Files in `~/.graphit/` |
| Hub uninstall language | Remove from project | Remove from global **only if no other project uses it** |
| Hub update language | Reinstall in project | Reinstall in global |
| `graphit sync` | No language re-materialize | Ensures global files up to date |
| Manual project YAML | Works (project precedence) | Still works (unchanged) |

## Resolution Chain (unchanged)

```
project > user global > runtime
```

- Project-level files in `.graphit/ast/queries/` still take precedence
- Global files in `~/.graphit/ast/queries/` are the second-priority source
- Runtime files in `~/.graphit/runtime/<version>/ast/queries/` are the fallback
