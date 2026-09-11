package memory

import (
	"context"
	"os"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func resolveScopeIDIn(projectDir, scope string) string {
	switch scope {
	case "project":
		return store.ProjectID(projectDir)
	case "user":
		userID, err := UserScopeIDForContext(context.Background())
		if err != nil {
			return ""
		}
		return userID
	default:
		return scope
	}
}

func resolveScopeID(scope string) string {
	wd, _ := os.Getwd()
	return resolveScopeIDIn(wd, scope)
}

// MemoryTableURI maps a validated logical scope to its authoritative store. Anonymous user memory
// is always local, even when Hub S3 is configured.
func MemoryTableURI(scopePath, localDir string) string {
	parts := strings.Split(strings.Trim(scopePath, "/"), "/")
	if len(parts) != 3 || parts[0] != "memory" {
		return ""
	}
	if cfg := memoryS3Config(context.Background(), parts); cfg.Configured() {
		if cfg.ResolutionError != nil {
			return ""
		}
		var prefix string
		switch parts[1] {
		case "project":
			if hubaccess.ValidateProjectID(parts[2]) != nil {
				return ""
			}
			prefix = hubaccess.ProjectMemoryPrefix(parts[2])
		case "user":
			if hubaccess.ValidateSubjectID("user", parts[2]) != nil {
				return ""
			}
			if hubaccess.IsAnonymousUserID(parts[2]) {
				return localDir
			}
			prefix = hubaccess.UserMemoryPrefix(parts[2])
		default:
			return ""
		}
		return s3store.URI(cfg.Bucket, s3store.JoinKey(cfg.Prefix, prefix))
	}
	return localDir
}

func memoryS3Config(ctx context.Context, parts []string) config.S3Config {
	if len(parts) != 3 || parts[0] != "memory" {
		return config.S3Config{}
	}
	switch parts[1] {
	case "project":
		return config.ProjectS3Config(ctx, parts[2])
	case "user":
		return config.UserS3Config(ctx)
	default:
		return config.S3Config{}
	}
}

func memoryS3ConfigForURI(ctx context.Context, uri string) config.S3Config {
	return config.S3ConfigForURI(ctx, uri)
}

// TableDirFor is the local table directory of a scope. It is named from the (scope, scopeID) pair
// so that a scope can never own two differently-named directories.
func TableDirFor(scope, scopeID string) string {
	return store.MemoryTableDir(scope, scopeID)
}

// TableURIFor is where the project or user scope's table lives, for callers outside the service.
func TableURIFor(scope, scopeID string) string {
	return MemoryTableURI("memory/"+scope+"/"+scopeID, TableDirFor(scope, scopeID))
}

// TableURIForScope is TableURIFor with the scope id resolved from the working directory.
func TableURIForScope(scope string) string {
	scopeID := resolveScopeID(scope)
	if scopeID == "" {
		return ""
	}
	return TableURIFor(scope, scopeID)
}

// ContextTableURI resolves an imported project's memory table by immutable project ID.
func ContextTableURI(projectID string) string {
	return MemoryTableURI("memory/project/"+projectID, TableDirFor(projectID, projectID))
}
