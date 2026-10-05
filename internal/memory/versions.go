package memory

import (
	"context"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// ProjectVersionBranches lists the native Lance branches of a project's memory table:
// one per project version other than the default that has written memory.
func ProjectVersionBranches(ctx context.Context, projectID string) ([]string, error) {
	uri := MemoryTableURIWithContext(ctx, "memory/project/"+projectID, TableDirFor(string(MemoryScopeProject), projectID))
	if uri == "" {
		return nil, nil
	}
	s3cfg := memoryS3ConfigForURI(ctx, uri)
	if err := authorizeMemoryURI(ctx, uri, s3cfg); err != nil {
		return nil, err
	}
	return lancestore.TableBranches(ctx, lancestore.Config{URI: uri, S3: s3cfg}, memoryTableName)
}
