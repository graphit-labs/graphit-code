package task

import (
	"context"
	"fmt"

	"github.com/graphit-labs/graphit-code/internal/lancestore"
)

// VersionBranches lists the native Lance branches of the project's Task store: one
// per project version other than the default that has written tasks or sessions.
func (s *Service) VersionBranches(ctx context.Context) ([]string, error) {
	if err := s.ensureIdentity(ctx); err != nil {
		return nil, err
	}
	projectID, uri := s.identity()
	if projectID == "" || uri == "" {
		return nil, fmt.Errorf("project is not initialized")
	}
	ctx, cancel := boundedContext(ctx, s.operationTimeout)
	defer cancel()
	if err := s.authorizeRemote(ctx, projectID); err != nil {
		return nil, err
	}
	branches, err := lancestore.TableBranches(ctx, lancestore.Config{URI: uri, S3: s.s3}, tasksTableName)
	if err != nil {
		return nil, taskStorageError(ctx, err)
	}
	return branches, nil
}
