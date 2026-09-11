package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
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

	"github.com/graphit-labs/graphit-code/internal/auth"
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
	brokerACL *auth.BrokerHubAccessClient
	broker    bool
	brokerID  string
	mu        sync.Mutex
	scoped    map[string]scopedS3Store
}

type scopedS3Store struct {
	objects *s3store.Store
	cfg     config.S3Config
}

func (s *S3Store) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

// NewS3Store builds the authoritative object store and its non-authoritative cache location.
func NewS3Store(ctx context.Context, inlineCfg, projectCfg config.ConfigMap) (*S3Store, error) {
	cacheDir, err := config.HubRepoDirPath()
	if err != nil {
		return nil, fmt.Errorf("resolving hub cache directory: %w", err)
	}

	cfg := config.ResolveHubS3(inlineCfg, projectCfg)
	hubStore := &S3Store{cfg: cfg, cacheBase: cacheDir, scoped: map[string]scopedS3Store{}}
	if snapshot, activeErr := auth.ResolveActive(ctx); activeErr == nil && snapshot.Provider.Type == auth.ProviderBroker {
		hubStore.broker = true
		hubStore.brokerID = auth.BrokerStorageIdentity(snapshot)
		cfg = config.HubMetadataS3Config(ctx)
		hubStore.cfg = cfg
	}
	if !cfg.Configured() {
		return hubStore, nil
	}
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		if errors.Is(err, s3store.ErrNotConfigured) {
			return hubStore, nil
		}
		return nil, err
	}
	hubStore.objects = objects
	if hubStore.broker {
		hubStore.scoped["hub"] = scopedS3Store{objects: objects, cfg: cfg}
	}
	brokerACL, accessErr := auth.NewBrokerHubAccessClient(ctx, nil)
	if accessErr != nil && !errors.Is(accessErr, auth.ErrBrokerHubAccessUnavailable) {
		return nil, fmt.Errorf("discover broker Hub authorization: %w", accessErr)
	}
	hubStore.brokerACL = brokerACL
	return hubStore, nil
}

func (s *S3Store) ResolveAccess(ctx context.Context) (hubaccess.Grants, hubaccess.Subject, string, error) {
	if s.brokerACL == nil {
		grants, subject, err := hubaccess.ResolveTrusted(ctx, s)
		return grants, subject, "projects-json", err
	}
	response, err := s.brokerACL.Resolve(ctx)
	if err != nil {
		return hubaccess.Grants{}, hubaccess.Subject{}, "", err
	}
	selectors := make([]hubaccess.Selector, len(response.Selectors))
	for i, selector := range response.Selectors {
		selectors[i] = hubaccess.Selector{ID: selector.ID, NamePrefix: selector.NamePrefix, All: selector.All}
	}
	grants, err := hubaccess.FromSelectors(selectors)
	if err != nil {
		return hubaccess.Grants{}, hubaccess.Subject{}, "", fmt.Errorf("invalid broker Hub access response: %w", err)
	}
	localSubject, err := hubaccess.TrustedSubject(ctx)
	if err != nil {
		return hubaccess.Grants{}, hubaccess.Subject{}, "", err
	}
	subject := brokerCacheSubject(response.Subject, localSubject)
	return grants, subject, response.AuthorizationRevision, nil
}

func brokerCacheSubject(canonical string, local hubaccess.Subject) hubaccess.Subject {
	if hubaccess.IsAnonymousUserID(local.UserID) {
		return hubaccess.Subject{UserID: hubaccess.AnonymousUserID}
	}
	digest := sha256.Sum256([]byte(strings.TrimSpace(canonical)))
	return hubaccess.Subject{UserID: "broker-" + hex.EncodeToString(digest[:16])}
}

func (s *S3Store) Authorize(ctx context.Context, projectID, normalizedName string) error {
	if s.brokerACL == nil {
		return hubaccess.Authorize(ctx, s, projectID, normalizedName)
	}
	grants, _, _, err := s.ResolveAccess(ctx)
	if err != nil {
		return err
	}
	if !grants.Allows(projectID, normalizedName) {
		return fmt.Errorf("%w: %s", hubaccess.ErrDenied, projectID)
	}
	return nil
}

func (s *S3Store) AuthorizeProject(ctx context.Context, projectID string) error {
	if s.brokerACL == nil {
		return hubaccess.AuthorizeProject(ctx, s, projectID)
	}
	if err := hubaccess.ValidateProjectID(projectID); err != nil {
		return err
	}
	grants, _, _, err := s.ResolveAccess(ctx)
	if err != nil {
		return err
	}
	if !grants.All() {
		allowed := false
		for _, id := range grants.ExactIDs() {
			if id == projectID {
				allowed = true
				break
			}
		}
		if !allowed {
			return fmt.Errorf("%w: %s", hubaccess.ErrDenied, projectID)
		}
	}
	data, err := s.Get(ctx, hubaccess.ProjectMetadataKey(projectID))
	if err != nil {
		return err
	}
	var document struct {
		Version int `json:"v"`
		Project struct {
			ID, Name, Status string
		} `json:"project"`
	}
	if json.Unmarshal(data, &document) != nil || document.Version != 2 || document.Project.ID != projectID || document.Project.Status != "active" {
		return fmt.Errorf("invalid project metadata for %s", projectID)
	}
	name, err := hubaccess.NormalizeProjectName(document.Project.Name)
	if err != nil || name != document.Project.Name {
		return fmt.Errorf("invalid project metadata name for %s", projectID)
	}
	return s.Authorize(ctx, projectID, name)
}

// Configured reports whether there is a remote at all.
func (s *S3Store) Configured() bool {
	if !s.broker {
		return s.objects != nil
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.objects != nil
}

// CacheDir is the non-authoritative Hub metadata cache root.
func (s *S3Store) CacheDir() string { return s.cacheBase }

// Bucket is the configured bucket, empty in local-only mode.
func (s *S3Store) Bucket() string {
	if !s.broker {
		return s.cfg.Bucket
	}
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.cfg.Bucket
}

func (s *S3Store) scopeStore(ctx context.Context, scope auth.BrokerStorageScope) (scopedS3Store, error) {
	if !s.broker {
		return scopedS3Store{objects: s.objects, cfg: s.cfg}, nil
	}
	snapshot, err := auth.ResolveActive(ctx)
	if err != nil {
		return scopedS3Store{}, err
	}
	identity := auth.BrokerStorageIdentity(snapshot)
	key := scope.Kind + "\x00" + scope.ProjectID
	s.mu.Lock()
	if identity != s.brokerID {
		s.scoped = map[string]scopedS3Store{}
		s.brokerID = identity
	}
	if cached, ok := s.scoped[key]; ok {
		s.mu.Unlock()
		return cached, nil
	}
	s.mu.Unlock()
	var cfg config.S3Config
	switch scope.Kind {
	case "project":
		cfg = config.ProjectS3Config(ctx, scope.ProjectID)
	case "user":
		cfg = config.UserS3Config(ctx)
	default:
		cfg = config.HubMetadataS3Config(ctx)
	}
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		return scopedS3Store{}, err
	}
	resolved := scopedS3Store{objects: objects, cfg: cfg}
	s.mu.Lock()
	if identity != s.brokerID {
		s.mu.Unlock()
		return s.scopeStore(ctx, scope)
	}
	if cached, ok := s.scoped[key]; ok {
		s.mu.Unlock()
		return cached, nil
	}
	s.scoped[key] = resolved
	if scope.Kind == "hub" {
		s.objects, s.cfg = resolved.objects, resolved.cfg
	}
	s.mu.Unlock()
	return resolved, nil
}

func (s *S3Store) storeForKey(ctx context.Context, key string) (scopedS3Store, error) {
	parts := strings.Split(strings.Trim(key, "/"), "/")
	for i := 0; i+2 < len(parts); i++ {
		if parts[i] != hubaccess.VersionPrefix {
			continue
		}
		switch parts[i+1] {
		case "projects":
			return s.scopeStore(ctx, auth.ProjectStorageScope(parts[i+2]))
		case "users":
			return s.scopeStore(ctx, auth.UserStorageScope())
		}
	}
	return s.scopeStore(ctx, auth.HubStorageScope())
}

func (s *S3Store) projectStore(ctx context.Context, projectID string) (scopedS3Store, error) {
	return s.scopeStore(ctx, auth.ProjectStorageScope(projectID))
}

// EnsureReachable verifies that the broker can authorize a minimal object-store request.
// In local-only mode there is nothing to reach and nothing to report.
func (s *S3Store) EnsureReachable(ctx context.Context) error {
	if !s.Configured() {
		return nil
	}
	storage, err := s.scopeStore(ctx, auth.HubStorageScope())
	if err != nil {
		return err
	}
	return storage.objects.EnsureBucket(ctx)
}

// ReadFile reads one logical v2 object key. The broker maps it to private physical storage.
func (s *S3Store) ReadFile(ctx context.Context, relPath string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return nil, err
	}
	return storage.objects.Get(ctx, relPath)
}

func (s *S3Store) Get(ctx context.Context, relPath string) ([]byte, error) {
	return s.ReadFile(ctx, relPath)
}

func (s *S3Store) ReadValue(ctx context.Context, relPath string) (s3store.Value, error) {
	if !s.Configured() {
		return s3store.Value{}, s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return s3store.Value{}, err
	}
	return storage.objects.GetValue(ctx, relPath)
}

// ReadArtifactFile reauthorizes and reads one object within its publisher project.
func (s *S3Store) ReadArtifactFile(ctx context.Context, projectID, key string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	if err := s.AuthorizeProject(ctx, projectID); err != nil {
		return nil, err
	}
	if err := validateProjectObjectKey(projectID, key); err != nil {
		return nil, err
	}
	storage, err := s.projectStore(ctx, projectID)
	if err != nil {
		return nil, err
	}
	return storage.objects.Get(ctx, key)
}

func (s *S3Store) writeArtifactFile(ctx context.Context, projectID, key string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := s.AuthorizeProject(ctx, projectID); err != nil {
		return err
	}
	if err := validateProjectObjectKey(projectID, key); err != nil {
		return err
	}
	storage, err := s.projectStore(ctx, projectID)
	if err != nil {
		return err
	}
	return storage.objects.Put(ctx, key, data)
}

func validateProjectObjectKey(projectID, key string) error {
	root := hubaccess.ProjectRoot(projectID)
	if root == "" || !strings.HasPrefix(key, root+"/") {
		return fmt.Errorf("object key %q is outside project %s", key, projectID)
	}
	return nil
}

func (s *S3Store) lanceConfig(uri string, writable bool) lancestore.Config {
	s3Config := s.cfg
	if s.broker {
		s3Config = config.S3ConfigForURI(context.Background(), uri)
	}
	return lancestore.Config{URI: uri, S3: s3Config, Writable: writable}
}

func (s *S3Store) WriteFile(ctx context.Context, relPath string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return err
	}
	return storage.objects.Put(ctx, relPath, data)
}

func (s *S3Store) RemoveFile(ctx context.Context, relPath string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return err
	}
	return storage.objects.Delete(ctx, relPath)
}

func (s *S3Store) WriteFileIfAbsent(ctx context.Context, relPath string, data []byte) (string, error) {
	if !s.Configured() {
		return "", s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return "", err
	}
	return storage.objects.PutIfAbsent(ctx, relPath, data)
}

func (s *S3Store) WriteFileIfMatch(ctx context.Context, relPath string, data []byte, etag string) (string, error) {
	if !s.Configured() {
		return "", s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return "", err
	}
	return storage.objects.PutIfMatch(ctx, relPath, data, etag)
}

func (s *S3Store) RemoveFileIfMatch(ctx context.Context, relPath, etag string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, relPath)
	if err != nil {
		return err
	}
	return storage.objects.DeleteIfMatch(ctx, relPath, etag)
}

func (s *S3Store) ListPage(ctx context.Context, prefix string, limit int, cursor string) (s3store.Page, error) {
	if !s.Configured() {
		return s3store.Page{}, s3store.ErrNotConfigured
	}
	storage, err := s.storeForKey(ctx, prefix)
	if err != nil {
		return s3store.Page{}, err
	}
	return storage.objects.ListPage(ctx, prefix, limit, cursor)
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

// ArtifactURI is the s3:// location mounted by LanceDB and Ladybug.
func (s *S3Store) ArtifactURI(artType ArtifactType, id, version, projectID string, parts ...string) string {
	if !s.Configured() {
		return ""
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return ""
	}
	var objects *s3store.Store
	if s.broker {
		s.mu.Lock()
		storage, ok := s.scoped["project\x00"+projectID]
		s.mu.Unlock()
		if !ok {
			return ""
		}
		objects = storage.objects
	} else {
		objects = s.objects
	}
	return objects.URI(s3store.JoinKey(append([]string{prefix}, parts...)...))
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
	if err := s.AuthorizeProject(ctx, projectID); err != nil {
		return err
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return fmt.Errorf("publishing %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	storage, err := s.projectStore(ctx, projectID)
	if err != nil {
		return err
	}
	if err := storage.objects.UploadDir(ctx, srcDir, prefix); err != nil {
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
	if err := s.AuthorizeProject(ctx, projectID); err != nil {
		return err
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return fmt.Errorf("publishing %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	storage, err := s.projectStore(ctx, projectID)
	if err != nil {
		return err
	}
	wanted := map[string]bool{}
	err = filepath.Walk(srcDir, func(path string, info os.FileInfo, walkErr error) error {
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
		if err := storage.objects.Put(ctx, key, data); err != nil {
			return err
		}
		wanted[key] = true
		return nil
	})
	if err != nil {
		return fmt.Errorf("publishing branch files for %s %s@%s: %w", artType, id, version, err)
	}
	objects, err := storage.objects.List(ctx, prefix)
	if err != nil {
		return err
	}
	for _, object := range objects {
		rel := strings.TrimPrefix(strings.TrimPrefix(object.Key, prefix), "/")
		if artifactPathIsLance(rel) || rel == branchHistoryFile || wanted[object.Key] {
			continue
		}
		if err := storage.objects.Delete(ctx, object.Key); err != nil {
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
	if err := s.AuthorizeProject(ctx, projectID); err != nil {
		return err
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return fmt.Errorf("deleting %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	storage, err := s.projectStore(ctx, projectID)
	if err != nil {
		return err
	}
	if err := storage.objects.DeletePrefix(ctx, prefix); err != nil {
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

// EnsureArtifactLocal downloads file-based artifacts; query-engine artifacts remain mounted.
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
	if err := s.AuthorizeProject(ctx, projectID); err != nil {
		return "", err
	}

	prefix := ArtifactPrefix(artType, id, version, projectID)
	if prefix == "" {
		return "", fmt.Errorf("downloading %s %s@%s: valid project ULID and artifact ID are required", artType, id, version)
	}
	storage, err := s.projectStore(ctx, projectID)
	if err != nil {
		return "", err
	}
	objs, err := storage.objects.List(ctx, prefix)
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
	if err := storage.objects.DownloadPrefix(ctx, prefix, staging); err != nil {
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
		s.log().Debug("event dropped, no broker storage configured", "key", key)
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
	requestBearer := auth.RequestBrokerBearer(ctx)

	pendingEvents.Add(1)
	go func() {
		defer pendingEvents.Done()

		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()
		if requestBearer != "" {
			ctx = auth.WithBrokerBearer(ctx, requestBearer)
		}
		ctx, err := hubaccess.WithTrustedSubject(ctx, subject)
		if err != nil {
			return
		}
		if err := s.AuthorizeProject(ctx, projectID); err != nil {
			s.log().Debug("event upload denied", "key", objectKey, "error", err)
			return
		}

		storage, resolveErr := s.projectStore(ctx, projectID)
		if resolveErr != nil {
			s.log().Debug("event storage resolution failed", "key", objectKey, "error", resolveErr)
			return
		}
		if err := storage.objects.Put(ctx, objectKey, payload); err != nil {
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
	key := s3store.JoinKey(hubaccess.GlobalRulesPrefix(), name)
	storage, err := s.storeForKey(ctx, key)
	if err != nil {
		return nil, err
	}
	return storage.objects.Get(ctx, key)
}

// ListRules names the rule overrides the Hub publishes.
func (s *S3Store) ListRules(ctx context.Context) ([]string, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	prefix := hubaccess.GlobalRulesPrefix()
	storage, err := s.storeForKey(ctx, prefix)
	if err != nil {
		return nil, err
	}
	objs, err := storage.objects.List(ctx, prefix)
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
	key := s3store.JoinKey(hubaccess.GlobalRulesPrefix(), name)
	storage, err := s.storeForKey(ctx, key)
	if err != nil {
		return err
	}
	return storage.objects.Put(ctx, key, data)
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
