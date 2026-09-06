package task

import (
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
)

// TableURI is the one authoritative task database for a project. When the Hub
// bucket exists every agent opens the S3 tables directly; there is no replica or
// synchronization layer.
func TableURI(projectID string, projectCfg config.ConfigMap) string {
	prefix := config.ResolveTaskPrefix(nil, projectCfg)
	if cfg := config.ResolveHubS3(nil, projectCfg); cfg.Configured() {
		projectPrefix := hubaccess.ProjectTaskPrefix(projectID, prefix)
		if projectPrefix == "" {
			return ""
		}
		return s3store.URI(cfg.Bucket, s3store.JoinKey(cfg.Prefix, projectPrefix))
	}
	localPrefix := strings.NewReplacer("/", "-", "\\", "-").Replace(prefix)
	return filepath.Join(store.TaskTableRoot(), store.SanitizeName(localPrefix), store.SanitizeSegment(projectID))
}
