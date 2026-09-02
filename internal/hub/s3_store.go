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

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

// The Hub's backend. Key convention and document schemas: docs/specs/hub-s3-object-layout.md.
const (
	registryPrefix = "registry"
	artifactPrefix = "artifacts"
	eventsPrefix   = "events"
	rulesPrefix    = "rules"

	// eventsStagingSubdir holds ONLY events whose upload failed, for the next flush to retry.
	// Normal traffic never lands here — see the telemetry section.
	eventsStagingSubdir = "events-staging"
)

// mountableTypes are published for the query engines to read in place. Everything else is
// downloaded, because the IDE reads it from disk.
var mountableTypes = map[ArtifactType]bool{
	TypeAST:       true,
	TypeKnowledge: true,
}

// S3Store is the Hub's persistence and retrieval backend.
//
// It replaces GitStore. The five things the git repository carried — the registry, one
// orphan branch per artifact version, refs/events/* telemetry, rule distribution, and
// memory stores — are all key prefixes here.
//
// Two properties differ from git in ways callers can feel:
//
//   - There is no commit. Every write is durable when it returns, so there is nothing to
//     push and no working tree to be dirty. What used to be atomicity across a commit is
//     now ordering: an artifact's prefix is uploaded BEFORE the registry entry names it.
//   - There is no clone. A mountable artifact is never downloaded; the engines read the
//     prefix over the network.
type S3Store struct {
	Logger *slog.Logger

	objects   *s3store.Store
	cfg       config.S3Config
	cacheBase string
}

func (s *S3Store) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

// NewS3Store builds the store from the resolved Hub configuration.
//
// A missing bucket is local-only mode, not an error: it returns a store whose Configured()
// is false, so every caller can keep working against the local cache. That mirrors what an
// unset hub.repo did.
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

// CacheDir is where downloaded artifacts and staged events live.
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

// ---------- registry mirror ----------
//
// The registry is small JSON metadata, and the code that reads it walks a directory. So it
// is mirrored locally, exactly as the git clone used to be, and AbsPath keeps working.
//
// This does NOT reintroduce the download the migration removed: what must never be
// transferred is the heavy half — the graph and the search index of a mountable artifact —
// and those are read in place from their own prefix.

// RegistryMirrorDir is the local copy of the registry prefix.
func (s *S3Store) RegistryMirrorDir() string {
	return filepath.Join(s.cacheBase, registryPrefix)
}

// AbsPath is where one registry document sits in the local mirror.
func (s *S3Store) AbsPath(relPath string) string {
	return filepath.Join(s.RegistryMirrorDir(), filepath.FromSlash(relPath))
}

// SyncRegistry refreshes the local mirror from the bucket.
//
// It replaces the whole mirror rather than merging, so a document deleted remotely
// disappears locally too — a stale entry left behind would advertise a version that no
// longer has a prefix, which is the one inconsistency this layout must not produce.
func (s *S3Store) SyncRegistry(ctx context.Context) error {
	if !s.Configured() {
		return nil
	}
	staging := s.RegistryMirrorDir() + ".partial"
	if err := os.RemoveAll(staging); err != nil {
		return err
	}
	// Created up front, not by the download. DownloadPrefix writes one file per object and makes
	// their parent directories on the way, so an EMPTY registry — a bucket nobody has published to
	// yet, which is every bucket right after setup — leaves nothing behind and the rename below
	// fails with "no such file or directory". An empty registry is a normal state, not an error.
	if err := os.MkdirAll(staging, 0o755); err != nil {
		return err
	}
	if err := s.objects.DownloadPrefix(ctx, registryPrefix, staging); err != nil {
		return fmt.Errorf("syncing registry: %w", err)
	}
	if err := os.MkdirAll(filepath.Dir(s.RegistryMirrorDir()), 0o755); err != nil {
		return err
	}
	if err := os.RemoveAll(s.RegistryMirrorDir()); err != nil {
		return err
	}
	return os.Rename(staging, s.RegistryMirrorDir())
}

// RegistryRevision identifies the registry's current state, so a cache built from it can be
// reused without rereading every document.
//
// It replaces the git HEAD commit. One listing over a prefix of small JSON files is cheap,
// and hashing key+size detects an added, removed, or rewritten document. It does NOT detect
// a same-size rewrite of one document — accepted: entry files are written by this code with
// a version in the name, so a same-size in-place change is not a case that occurs.
func (s *S3Store) RegistryRevision(ctx context.Context) string {
	if !s.Configured() {
		return ""
	}
	objs, err := s.objects.List(ctx, registryPrefix)
	if err != nil {
		return ""
	}
	lines := make([]string, 0, len(objs))
	for _, o := range objs {
		lines = append(lines, fmt.Sprintf("%s:%d", o.Key, o.Size))
	}
	sort.Strings(lines)
	sum := sha256.Sum256([]byte(strings.Join(lines, "\n")))
	return hex.EncodeToString(sum[:])
}

// ---------- registry documents ----------

// ReadFile reads one registry document.
func (s *S3Store) ReadFile(ctx context.Context, relPath string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	return s.objects.Get(ctx, s3store.JoinKey(registryPrefix, relPath))
}

// ReadArtifactFile reads ONE object out of an artifact prefix, by its own key.
//
// Separate from ReadFile because that one prefixes with the registry's namespace: an artifact key
// passed to it resolves under `registry/`, which does not exist and fails with a not-found that
// names a path nobody wrote. Kept narrow on purpose — this exists so a MOUNTED artifact can fetch
// its schema without fetching its data, and widening it into a general download would give back
// the door the migration closed.
func (s *S3Store) ReadArtifactFile(ctx context.Context, key string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	return s.objects.Get(ctx, key)
}

// WriteFile writes one registry document, remotely and into the local mirror.
//
// It is durable on return — there is no commit step. Writing both sides here is what keeps a
// read immediately after a publish consistent, which is what the commit used to guarantee.
func (s *S3Store) WriteFile(ctx context.Context, relPath string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := s.objects.Put(ctx, s3store.JoinKey(registryPrefix, relPath), data); err != nil {
		return err
	}
	local := s.AbsPath(relPath)
	if err := os.MkdirAll(filepath.Dir(local), 0o755); err != nil {
		return err
	}
	return os.WriteFile(local, data, 0o644)
}

// RemoveFile deletes one registry document, remotely and from the local mirror.
func (s *S3Store) RemoveFile(ctx context.Context, relPath string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	if err := s.objects.Delete(ctx, s3store.JoinKey(registryPrefix, relPath)); err != nil {
		return err
	}
	if err := os.Remove(s.AbsPath(relPath)); err != nil && !os.IsNotExist(err) {
		return err
	}
	return nil
}

// ListDir names the registry documents under relPath, relative to it.
//
// It lists a prefix, so it returns entries at any depth below relPath rather than only the
// immediate children — the registry's own layout is two levels of hash fan-out and the
// callers walk all of it.
func (s *S3Store) ListDir(ctx context.Context, relPath string) ([]string, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	objs, err := s.objects.List(ctx, s3store.JoinKey(registryPrefix, relPath))
	if err != nil {
		return nil, err
	}
	base := s3store.JoinKey(registryPrefix, relPath)
	out := make([]string, 0, len(objs))
	for _, o := range objs {
		out = append(out, strings.TrimPrefix(strings.TrimPrefix(o.Key, base), "/"))
	}
	sort.Strings(out)
	return out, nil
}

// ---------- artifacts ----------

// ArtifactPrefix is the key prefix of one published artifact version.
//
// The segments are the ones the orphan-branch layout used, minus the branch namespace: type
// folder, project, id, version. ast and knowledge omit the id because a project publishes
// exactly one of each.
func ArtifactPrefix(artType ArtifactType, id, version, projectID string) string {
	folder := TypeFolderMap[artType]
	if folder == "" {
		folder = string(artType)
	}
	project := projectID
	if project == "" {
		project = globalProjectKey
	}
	if mountableTypes[artType] {
		return s3store.JoinKey(artifactPrefix, folder, project, version)
	}
	return s3store.JoinKey(artifactPrefix, folder, project, id, version)
}

// ArtifactURI is the s3:// location a query engine mounts.
//
// This is the whole point of the migration for mountable types: installing records this URI
// and runs the mount DDL, and no bytes of graph or index are transferred.
func (s *S3Store) ArtifactURI(artType ArtifactType, id, version, projectID string, parts ...string) string {
	if !s.Configured() {
		return ""
	}
	return s.objects.URI(s3store.JoinKey(append([]string{ArtifactPrefix(artType, id, version, projectID)}, parts...)...))
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
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if err := s.objects.UploadDir(ctx, srcDir, prefix); err != nil {
		return fmt.Errorf("publishing %s %s@%s: %w", artType, id, version, err)
	}
	return nil
}

// DeleteArtifact removes a published version entirely.
func (s *S3Store) DeleteArtifact(ctx context.Context, artType ArtifactType, id, version, projectID string) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	prefix := ArtifactPrefix(artType, id, version, projectID)
	if err := s.objects.DeletePrefix(ctx, prefix); err != nil {
		return fmt.Errorf("deleting %s %s@%s: %w", artType, id, version, err)
	}
	return nil
}

// ArtifactCacheDir is where a downloaded artifact lands.
func (s *S3Store) ArtifactCacheDir(artType ArtifactType, id, version, projectID string) string {
	return ArtifactCacheDirIn(s.cacheBase, artType, id, version, projectID)
}

// ArtifactCacheDirIn computes the same path without a store, for callers that need the
// location and nothing else.
func ArtifactCacheDirIn(cacheBase string, artType ArtifactType, id, version, projectID string) string {
	return filepath.Join(cacheBase, "artifacts", filepath.FromSlash(ArtifactPrefix(artType, id, version, projectID)))
}

// EnsureArtifactLocal downloads a file-based artifact and returns its directory.
//
// A published version is immutable, so an existing non-empty cache directory is reused
// without touching the network.
//
// It refuses a mountable type rather than downloading it: ast and knowledge are read in
// place, and materialising them here would silently reintroduce the download this migration
// removed.
func (s *S3Store) EnsureArtifactLocal(ctx context.Context, artType ArtifactType, id, version, projectID string) (string, error) {
	if mountableTypes[artType] {
		return "", fmt.Errorf("%s artifacts are mounted from %s, not downloaded — use ArtifactURI",
			artType, s.ArtifactURI(artType, id, version, projectID))
	}
	return s.DownloadArtifact(ctx, artType, id, version, projectID)
}

// DownloadArtifact materialises an artifact prefix locally, including a mountable one.
//
// TODO(T9): the mountable types must stop being downloaded — see
// docs/tasks/hub-em-s3-icebug-e-lancedb.md. Until the graph is mounted with
// `storage = 's3://…'` and the search index is opened from its prefix, installing an ast or
// knowledge context still needs the bytes, and this is the one door that provides them.
// EnsureArtifactLocal is the destination behaviour and refuses them; this is the exception
// that has to disappear, which is why it is named separately instead of being a flag.
func (s *S3Store) DownloadArtifact(ctx context.Context, artType ArtifactType, id, version, projectID string) (string, error) {
	dest := s.ArtifactCacheDir(artType, id, version, projectID)
	if entries, err := os.ReadDir(dest); err == nil && len(entries) > 0 {
		return dest, nil
	}
	if !s.Configured() {
		return "", s3store.ErrNotConfigured
	}

	prefix := ArtifactPrefix(artType, id, version, projectID)
	objs, err := s.objects.List(ctx, prefix)
	if err != nil {
		return "", err
	}
	if len(objs) == 0 {
		return "", fmt.Errorf("%s %s@%s: no objects under %s: %w", artType, id, version, prefix, s3store.ErrNotFound)
	}

	// Download beside the destination and move into place, so an interrupted download does
	// not leave a partial directory that the reuse check above would then trust forever.
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

// ---------- telemetry ----------
//
// AN EVENT IS UPLOADED WHEN IT HAPPENS. It is not queued.
//
// It used to be staged on disk and drained later, and that made sense against git: an event was a
// write under refs/events/* plus a push, which is expensive enough per event that batching earned
// its keep. Against object storage it earns nothing — SyncEvents did one Put PER EVENT, so the
// queue deferred the same number of requests instead of reducing them, and it was drained from
// exactly one place (`graphit sync`), so events from every other command accumulated on disk until
// somebody happened to run it.
//
// So the staging directory is now the FAILURE path only: an event that cannot be uploaded is kept
// so the next flush can retry it, and nothing else is ever written there. With no bucket configured
// there is no destination at all and the event is dropped, because a queue with no consumer is a
// disk leak, not durability.

// maxStagedEvents bounds the failure path. A remote that is broken rather than briefly unreachable
// would otherwise grow the directory without limit, and telemetry is not worth unbounded disk.
const maxStagedEvents = 256

// stagedEvent is an event that failed to upload, with the key it was meant to have.
//
// The key is stored rather than recovered from the file name, and that fixes a real defect: the old
// code staged under `strings.ReplaceAll(key, "/", "_")` and rebuilt the key with the inverse
// replacement — but a key already contains underscores, in the ULID and in the action, so every
// retried event was uploaded under a mangled key.
type stagedEvent struct {
	Key  string `json:"key"`
	Body string `json:"body"`
}

var pendingEvents sync.WaitGroup

// WaitForPendingEvents blocks until every in-flight upload has finished. The CLI and the daemon
// call it on shutdown, so an event raised by the last command is not lost with the process.
func WaitForPendingEvents() { pendingEvents.Wait() }

// WriteEventFile uploads one event, in the background.
//
// It never returns an error and never blocks: telemetry that can fail or slow a user's command is
// worse than telemetry that is missing.
func (s *S3Store) WriteEventFile(key string, data []byte) {
	if !s.Configured() {
		// Nothing to send it to. Dropped rather than queued — see the note above.
		s.log().Debug("event dropped, no bucket configured", "key", key)
		return
	}

	objectKey := s3store.JoinKey(eventsPrefix, key)
	payload := append([]byte(nil), data...)

	pendingEvents.Add(1)
	go func() {
		defer pendingEvents.Done()

		ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
		defer cancel()

		if err := s.objects.Put(ctx, objectKey, payload); err != nil {
			s.log().Debug("event upload failed, staging for retry", "key", objectKey, "error", err)
			s.stageEvent(objectKey, payload)
		}
	}()
}

// stageEvent keeps an event that failed to upload, so the next flush can retry it.
func (s *S3Store) stageEvent(objectKey string, data []byte) {
	dir := s.eventsStagingDir()
	if err := os.MkdirAll(dir, 0o755); err != nil {
		s.log().Debug("events staging dir", "error", err)
		return
	}
	s.evictOldestStaged(dir)

	raw, err := json.Marshal(stagedEvent{Key: objectKey, Body: string(data)})
	if err != nil {
		return
	}
	// The file name only has to be unique; the key travels inside.
	name := fmt.Sprintf("%x.json", sha256.Sum256(append(raw, []byte(objectKey)...)))
	if err := os.WriteFile(filepath.Join(dir, name), raw, 0o644); err != nil {
		s.log().Debug("staging event", "error", err)
	}
}

// evictOldestStaged drops the oldest staged events until there is room for one more.
func (s *S3Store) evictOldestStaged(dir string) {
	entries, err := os.ReadDir(dir)
	if err != nil || len(entries) < maxStagedEvents {
		return
	}
	type aged struct {
		path string
		at   time.Time
	}
	files := make([]aged, 0, len(entries))
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		info, statErr := e.Info()
		if statErr != nil {
			continue
		}
		files = append(files, aged{path: filepath.Join(dir, e.Name()), at: info.ModTime()})
	}
	sort.Slice(files, func(i, j int) bool { return files[i].at.Before(files[j].at) })
	for i := 0; i+maxStagedEvents-1 < len(files); i++ {
		_ = os.Remove(files[i].path)
	}
}

// SyncEvents retries the events that failed to upload, and removes the ones that land.
//
// This is a retry drain, not a flush of normal traffic: in the healthy case there is nothing here,
// because WriteEventFile uploads as it goes. Each event is its own immutable object, so concurrent
// publishers never contend and a partial retry loses nothing.
func (s *S3Store) SyncEvents(ctx context.Context) {
	if !s.Configured() {
		return
	}
	dir := s.eventsStagingDir()
	entries, err := os.ReadDir(dir)
	if err != nil || len(entries) == 0 {
		return
	}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		path := filepath.Join(dir, e.Name())
		raw, readErr := os.ReadFile(path)
		if readErr != nil {
			continue
		}
		var ev stagedEvent
		if json.Unmarshal(raw, &ev) != nil || ev.Key == "" {
			// Not something this build wrote, and there is no key to send it under.
			_ = os.Remove(path)
			continue
		}
		if putErr := s.objects.Put(ctx, ev.Key, []byte(ev.Body)); putErr != nil {
			s.log().Debug("retrying event", "key", ev.Key, "error", putErr)
			continue
		}
		_ = os.Remove(path)
	}
}

func (s *S3Store) eventsStagingDir() string {
	return filepath.Join(s.cacheBase, eventsStagingSubdir)
}

// EventKey names one telemetry object. The timestamp leads so a listing is chronological.
func EventKey(projectID, artifactType, action string, at time.Time, unique string) string {
	project := projectID
	if project == "" {
		project = "_default"
	}
	kind := artifactType
	if kind == "" {
		kind = "_none"
	}
	return s3store.JoinKey(project, kind, at.UTC().Format("20060102T150405Z")+"_"+unique+"_"+action+".json")
}

// ---------- rules ----------

// ReadRule reads one team-wide rule override.
func (s *S3Store) ReadRule(ctx context.Context, name string) ([]byte, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	return s.objects.Get(ctx, s3store.JoinKey(rulesPrefix, name))
}

// ListRules names the rule overrides the Hub publishes.
func (s *S3Store) ListRules(ctx context.Context) ([]string, error) {
	if !s.Configured() {
		return nil, s3store.ErrNotConfigured
	}
	objs, err := s.objects.List(ctx, rulesPrefix)
	if err != nil {
		return nil, err
	}
	out := make([]string, 0, len(objs))
	for _, o := range objs {
		out = append(out, strings.TrimPrefix(strings.TrimPrefix(o.Key, rulesPrefix), "/"))
	}
	sort.Strings(out)
	return out, nil
}

// WriteRule publishes one rule override.
func (s *S3Store) WriteRule(ctx context.Context, name string, data []byte) error {
	if !s.Configured() {
		return s3store.ErrNotConfigured
	}
	return s.objects.Put(ctx, s3store.JoinKey(rulesPrefix, name), data)
}

// ---------- manifest helpers ----------

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
