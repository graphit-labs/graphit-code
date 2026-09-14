package lancestore

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
)

// ShallowSourceURI returns the remote source of a locally cloned Lance index.
// The URI is metadata only; callers resolve credentials from their active
// request context, so no S3 secret is persisted with the local clone.
func ShallowSourceURI(localStorePath string) string {
	data, err := os.ReadFile(filepath.Join(localStorePath, ".graphit-base.json"))
	if err != nil {
		return ""
	}
	var marker struct {
		SourceURI string `json:"source_uri"`
	}
	if json.Unmarshal(data, &marker) != nil || !strings.HasPrefix(marker.SourceURI, "s3://") {
		return ""
	}
	return marker.SourceURI
}
