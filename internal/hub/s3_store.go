package hub

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
)

var mountableTypes = map[ArtifactType]bool{
	TypeAST:       true,
	TypeKnowledge: true,
}

type S3Store struct {
	Logger *slog.Logger

	objects   *s3store.Store
	cfg       config.S3Config
	cacheBase string
}

func (s *S3Store) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

// NewS3Store builds the authoritative object store and its non-authoritative cache location.
func NewS3Store(ctx context.Context, inlineCfg, projectCfg config.ConfigMap) (*S3Store, error) {
	cacheDir, err := config.HubRepoDirPath()
	if err != nil {
		return nil, fmt.Errorf("resolving hub cache directory: %w", err)
	}

	cfg := config.ResolveHubS3(inlineCfg, projectCfg)
	store := &S3Store{cfg: cfg, cacheBase: cacheDir}

	if !cfg.Configured() {
		return store, nil
	}
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		if errors.Is(err, s3store.ErrNotConfigured) {
			return store, nil
		}
		return nil, err
	}
	store.objects = objects
	return store, nil
}

// Configured reports whether there is a remote at all.
func (s *S3Store) Configured() bool { return s.objects != nil }

// CacheDir is the non-authoritative Hub metadata cache root.
func (s *S3Store) CacheDir() string { return s.cacheBase }

// Bucket is the configured bucket, empty in local-only mode.
func (s *S3Store) Bucket() string { return s.cfg.Bucket }

// EnsureReachable verifies the bucket answers with the credentials the AWS chain resolved.
// In local-only mode there is nothing to reach and nothing to report.
func (s *S3Store) EnsureReachable(ctx context.Context) error {
	if !s.Configured() {
		return nil
	}
	return s.objects.EnsureBucket(ctx)
}

// ReadFile reads one fully qualified v2 object key relative to hub.prefix.
func (s *S3Store) ReadFile(ctx context.Context, relPath string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	return s.objects.Get(ctx, relPath)
}

func (s *S3Store) Get(ctx context.Context, relPath string) ([]byte, error) {
	return s.ReadFile(ctx, relPath)
}

func (s *S3Store) ReadValue(ctx context.Context, relPath string) (s3store.Value, error) {
	if !s.Configured() {
		return s3store.Value{}, s3store.ErrNotConfigured
	}
	return s.objects.GetValue(ctx, relPath)
}

// ReadArtifactFile reauthorizes and reads one object within its publisher project.
func (s *S3Store) ReadArtifactFile(ctx context.Context, projectID, key string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return nil, err
	}
	if err := validateProjectObjectKey(projectID, key); err != nil {
		return nil, err
	}
	return s.objects.Get(ctx, key)
}

func (s *S3Store) writeArtifactFile(ctx context.Context, projectID, key string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return err
	}
	if err := validateProjectObjectKey(projectID, key); err != nil {
		return err
	}
	return s.objects.Put(ctx, key, data)
}

func validateProjectObjectKey(projectID, key string) error {
	root := hubaccess.ProjectRoot(projectID)
	if root == "" || !strings.HasPrefix(key, root+"/") {
		return fmt.Errorf("object key %q is outside project %s", key, projectID)
	}
	return nil
}

func (s *S3Store) lanceConfig(uri string, writable bool) lancestore.Config {
	return lancestore.Config{URI: uri, S3: s.cfg, Writable: writable}
}

func (s *S3Store) WriteFile(ctx context.Context, relPath string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	return s.objects.Put(ctx, relPath, data)
}

func (s *S3Store) RemoveFile(ctx context.Context, relPath string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	return s.objects.Delete(ctx, relPath)
}

func (s *S3Store) WriteFileIfAbsent(ctx context.Context, relPath string, data []byte) (string, error) {
	if !s.Configured() {
		return "", s3store.ErrNotConfigured
	}
	return s.objects.PutIfAbsent(ctx, relPath, data)
}

func (s *S3Store) WriteFileIfMatch(ctx context.Context, relPath string, data []byte, etag string) (string, error) {
	if !s.Configured() {
		return "", s3store.ErrNotConfigured
	}
	return s.objects.PutIfMatch(ctx, relPath, data, etag)
}

func (s *S3Store) RemoveFileIfMatch(ctx context.Context, relPath, etag string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	return s.objects.DeleteIfMatch(ctx, relPath, etag)
}

func (s *S3Store) ListPage(ctx context.Context, prefix string, limit int, cursor string) (s3store.Page, error) {
	if !s.Configured() {
		return s3store.Page{}, s3store.ErrNotConfigured
	}
	return s.objects.ListPage(ctx, prefix, limit, cursor)
}

// ArtifactPrefix is the key prefix of one published artifact version.
func ArtifactPrefix(artType ArtifactType, id, version, projectID string) string {
	if hubaccess.ValidateProjectID(projectID) != nil || strings.TrimSpace(id) == "" {
		return ""
	}
	folder := TypeFolderMap[artType]
	if folder == "" {
		folder = string(artType)
	}
	version = store.VersionPathSegment(version)
	return hubaccess.ProjectArtifactPrefix(projectID, folder, id, version)
}

// ArtifactURI is the s3:// location a query engine mounts.
func (s *S3Store) ArtifactURI(artType ArtifactType, id, version, projectID string, parts ...string) string {
	if !s.Configured() {
		return ""
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return ""
	}
	return s.objects.URI(s3store.JoinKey(append([]string{prefix}, parts...)...))
}

// IsMountable reports whether this type is read in place rather than downloaded.
func IsMountable(artType ArtifactType) bool { return mountableTypes[artType] }

// PublishArtifact uploads srcDir as one artifact version.
//
// SAFETY: the caller must write the registry entry AFTER this returns, never before. The
// entry naming a version whose prefix is absent is the one inconsistency this layout cannot
// tolerate; the reverse — a prefix nothing points at — is only wasted bytes.
func (s *S3Store) PublishArtifact(ctx context.Context, artType ArtifactType, id, version, projectID, srcDir string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return err
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return fmt.Errorf("publishing %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	if err := s.objects.UploadDir(ctx, srcDir, prefix); err != nil {
		return fmt.Errorf("publishing %s %s@%s: %w", artType, id, version, err)
	}
	return nil
}

// PublishBranchFiles mirrors non-Lance files while preserving the branch's authoritative Lance
// datasets and commit history.
func (s *S3Store) PublishBranchFiles(ctx context.Context, artType ArtifactType, id, version, projectID, srcDir string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return err
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return fmt.Errorf("publishing %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	wanted := map[string]bool{}
	err := filepath.Walk(srcDir, func(path string, info os.FileInfo, walkErr error) error {
		if walkErr != nil {
			return walkErr
		}
		rel, err := filepath.Rel(srcDir, path)
		if err != nil {
			return err
		}
		rel = filepath.ToSlash(rel)
		if rel == "." {
			return nil
		}
		if artifactPathIsLance(rel) {
			if info.IsDir() {
				return filepath.SkipDir
			}
			return nil
		}
		if info.IsDir() || rel == branchHistoryFile {
			return nil
		}
		data, err := os.ReadFile(path)
		if err != nil {
			return err
		}
		key := s3store.JoinKey(prefix, rel)
		if err := s.objects.Put(ctx, key, data); err != nil {
			return err
		}
		wanted[key] = true
		return nil
	})
	if err != nil {
		return fmt.Errorf("publishing branch files for %s %s@%s: %w", artType, id, version, err)
	}
	objects, err := s.objects.List(ctx, prefix)
	if err != nil {
		return err
	}
	for _, object := range objects {
		rel := strings.TrimPrefix(strings.TrimPrefix(object.Key, prefix), "/")
		if artifactPathIsLance(rel) || rel == branchHistoryFile || wanted[object.Key] {
			continue
		}
		if err := s.objects.Delete(ctx, object.Key); err != nil {
			return err
		}
	}
	return nil
}

func artifactPathIsLance(rel string) bool {
	for _, segment := range strings.Split(filepath.ToSlash(rel), "/") {
		if strings.HasSuffix(segment, ".lance") {
			return true
		}
	}
	return false
}

// DeleteArtifact removes a published version entirely.
func (s *S3Store) DeleteArtifact(ctx context.Context, artType ArtifactType, id, version, projectID string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return err
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return fmt.Errorf("deleting %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	if err := s.objects.DeletePrefix(ctx, prefix); err != nil {
		return fmt.Errorf("deleting %s %s@%s: %w", artType, id, version, err)
	}
	return nil
}

// ArtifactCacheDir is where a downloaded artifact lands.
func (s *S3Store) ArtifactCacheDir(artType ArtifactType, id, version, projectID string) string {
	return ArtifactCacheDirIn(brand.GlobalDir(), artType, id, version, projectID)
}

// ArtifactCacheDirIn computes the same path without a store, for callers that need the
// location and nothing else.
func ArtifactCacheDirIn(globalRoot string, artType ArtifactType, id, version, projectID string) string {
	return filepath.Join(globalRoot, "artifacts", "modules", store.SanitizeSegment(projectID), string(artType), store.SanitizeSegment(id), store.VersionPathSegment(version))
}

// EnsureArtifactLocal downloads file-based artifacts; mountable types remain remote.
func (s *S3Store) EnsureArtifactLocal(ctx context.Context, artType ArtifactType, id, version, projectID string) (string, error) {
	if mountableTypes[artType] {
		return "", fmt.Errorf("%s artifacts are mounted from %s, not downloaded — use ArtifactURI",
			artType, s.ArtifactURI(artType, id, version, projectID))
	}
	return s.DownloadArtifact(ctx, artType, id, version, projectID)
}

// DownloadArtifact materialises an artifact prefix locally, including a mountable one.
func (s *S3Store) DownloadArtifact(ctx context.Context, artType ArtifactType, id, version, projectID string) (string, error) {
	dest := s.ArtifactCacheDir(artType, id, version, projectID)
	if !s.Configured() {
		return "", s3store.ErrNotConfigured
	}
	if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
		return "", err
	}

	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return "", fmt.Errorf("downloading %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	objs, err := s.objects.List(ctx, prefix)
	if err != nil {
		return "", err
	}
	if len(objs) == 0 {
		return "", fmt.Errorf("%s %s@%s: no objects under %s: %w", artType, id, version, prefix, s3store.ErrNotFound)
	}

	staging := dest + ".partial"
	if err := os.RemoveAll(staging); err != nil {
		return "", err
	}
	if err := os.MkdirAll(filepath.Dir(dest), 0o755); err != nil {
		return "", err
	}
	if err := s.objects.DownloadPrefix(ctx, prefix, staging); err != nil {
		return "", err
	}
	if err := os.RemoveAll(dest); err != nil {
		return "", err
	}
	if err := os.Rename(staging, dest); err != nil {
		return "", err
	}
	return dest, nil
}

var pendingEvents sync.WaitGroup

// WaitForPendingEvents blocks until every in-flight upload has finished. The CLI and the daemon
// call it on shutdown, so an event raised by the last command is not lost with the process.
func WaitForPendingEvents() { pendingEvents.Wait() }

// WriteEventFile uploads one event, in the background.
//
// It never returns an error and never blocks: telemetry that can fail or slow a user's command is
// worse than telemetry that is missing.
func (s *S3Store) WriteEventFile(ctx context.Context, projectID, key string, data []byte) {
	if !s.Configured() || key == "" {
		s.log().Debug("event dropped, no bucket configured", "key", key)
		return
	}
	subject, err := hubaccess.TrustedSubject(ctx)
	if err != nil {
		s.log().Debug("event dropped, no trusted subject", "key", key)
		return
	}
	if err := validateProjectObjectKey(projectID, key); err != nil {
		s.log().Debug("event dropped, invalid project key", "key", key, "error", err)
		return
	}

	objectKey := key
	payload := append([]byte(nil), data...)

	pendingEvents.Add(1)
	go func() {
		defer pendingEvents.Done()

		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		ctx, err := hubaccess.WithTrustedSubject(ctx, subject)
		if err != nil {
			return
		}
		if err := hubaccess.AuthorizeProject(ctx, s, projectID); err != nil {
			s.log().Debug("event upload denied", "key", objectKey, "error", err)
			return
		}

		if err := s.objects.Put(ctx, objectKey, payload); err != nil {
			s.log().Debug("event upload failed", "key", objectKey, "error", err)
		}
	}()
}

// EventKey names one telemetry object. The timestamp leads so a listing is chronological.
func EventKey(projectID, artifactType, action string, at time.Time, unique string) string {
	if hubaccess.ValidateProjectID(projectID) != nil {
		return ""
	}
	kind := artifactType
	if kind == "" {
		kind = "_none"
	}
	if !validEventSegment(kind) || !validEventSegment(action) || !validEventSegment(unique) {
		return ""
	}
	return s3store.JoinKey(hubaccess.ProjectEventsPrefix(projectID), kind, at.UTC().Format("20060102T150405Z")+"_"+unique+"_"+action+".json")
}

func validEventSegment(value string) bool {
	value = strings.TrimSpace(value)
	return value != "" && value != "." && value != ".." && !strings.ContainsAny(value, `/\\`)
}

// ReadRule reads one team-wide rule override.
func (s *S3Store) ReadRule(ctx context.Context, name string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	return s.objects.Get(ctx, s3store.JoinKey(hubaccess.GlobalRulesPrefix(), name))
}

// ListRules names the rule overrides the Hub publishes.
func (s *S3Store) ListRules(ctx context.Context) ([]string, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	prefix := hubaccess.GlobalRulesPrefix()
	objs, err := s.objects.List(ctx, prefix)
	if err != nil {
		return nil, err
	}
	out := make([]string, 0, len(objs))
	for _, o := range objs {
		out = append(out, strings.TrimPrefix(strings.TrimPrefix(o.Key, prefix), "/"))
	}
	sort.Strings(out)
	return out, nil
}

// WriteRule publishes one rule override.
func (s *S3Store) WriteRule(ctx context.Context, name string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	return s.objects.Put(ctx, s3store.JoinKey(hubaccess.GlobalRulesPrefix(), name), data)
}

// ReadJSON reads a registry document and refuses a manifest version this build does not
// know, rather than parsing it into a shape it may not have.
func ReadJSON[T any](ctx context.Context, s *S3Store, relPath string, out *T) error {
	data, err := s.ReadFile(ctx, relPath)
	if err != nil {
		return err
	}
	var probe struct {
		Version int `json:"v"`
	}
	if err := json.Unmarshal(data, &probe); err == nil && probe.Version > hubManifestVersion {
		return fmt.Errorf("%s declares manifest version %d, this build reads %d — written by a newer publisher",
			relPath, probe.Version, hubManifestVersion)
	}
	return json.Unmarshal(data, out)
}
