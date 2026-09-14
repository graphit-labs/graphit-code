package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/storelifecycle"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

const (
	branchHistoryFile    = "graphit-history.json"
	branchHistoryVersion = 1
	localBaseFile        = ".graphit-base.json"
)

// HydrationResult reports the published Git base selected on this sync.
// A non-empty AST commit lets the indexer reconcile only the checkout delta
// while retaining the shallow-cloned search tables.
type HydrationResult struct {
	ASTBaseCommit       string
	KnowledgeBaseCommit string
}

type localLanceBase struct {
	EntryID   string      `json:"entry_id"`
	Branch    string      `json:"branch"`
	SourceURI string      `json:"source_uri"`
	Base      lanceCommit `json:"base"`
}

type lanceTableRef struct {
	Version uint64 `json:"version"`
	Tag     string `json:"tag"`
}

type lanceCommit struct {
	Commit          string                   `json:"commit"`
	ProducerVersion string                   `json:"producer_version"`
	Fingerprint     string                   `json:"fingerprint"`
	PublishedAt     time.Time                `json:"published_at"`
	Tables          map[string]lanceTableRef `json:"tables"`
}

type lanceBranchHistory struct {
	Version      int           `json:"v"`
	ProjectID    string        `json:"project_id"`
	ArtifactType ArtifactType  `json:"artifact_type"`
	Branch       string        `json:"branch"`
	Commits      []lanceCommit `json:"commits"`
}

func lanceFingerprint(artType ArtifactType) string {
	provider, model, dimensions := ai.ConfiguredEmbeddingIdentity()
	format := map[ArtifactType]string{TypeAST: "ast-search-v1", TypeKnowledge: "knowledge-index-v1"}[artType]
	sum := sha256.Sum256([]byte(fmt.Sprintf("%s\x00%s\x00%s\x00%d", format, provider, model, dimensions)))
	return hex.EncodeToString(sum[:])
}

func lanceCommitTag(commit string) string { return "git-" + commit }

func selectLanceBase(history lanceBranchHistory, ancestry []string, fingerprint string) (lanceCommit, bool) {
	byCommit := make(map[string]lanceCommit, len(history.Commits))
	for _, commit := range history.Commits {
		if commit.Fingerprint == fingerprint {
			byCommit[commit.Commit] = commit
		}
	}
	for _, ancestor := range ancestry {
		if commit, ok := byCommit[ancestor]; ok {
			return commit, true
		}
	}
	return lanceCommit{}, false
}

func (s *S3Store) branchHistoryKey(artType ArtifactType, id, branchVersion, projectID string) string {
	return s3store.JoinKey(ArtifactPrefix(artType, id, branchVersion, projectID), branchHistoryFile)
}

func (s *S3Store) readBranchHistory(ctx context.Context, artType ArtifactType, id, branchVersion, projectID string) (lanceBranchHistory, error) {
	data, err := s.ReadArtifactFile(ctx, projectID, s.branchHistoryKey(artType, id, branchVersion, projectID))
	if err != nil {
		return lanceBranchHistory{}, err
	}
	var history lanceBranchHistory
	if err := json.Unmarshal(data, &history); err != nil {
		return lanceBranchHistory{}, fmt.Errorf("decode Lance branch history: %w", err)
	}
	if history.Version != branchHistoryVersion {
		return lanceBranchHistory{}, fmt.Errorf("unsupported Lance branch history version %d", history.Version)
	}
	return history, nil
}

func (s *S3Store) writeBranchHistory(ctx context.Context, artType ArtifactType, id, branchVersion, projectID string, history lanceBranchHistory) error {
	data, err := json.MarshalIndent(history, "", "  ")
	if err != nil {
		return err
	}
	return s.writeArtifactFile(ctx, projectID, s.branchHistoryKey(artType, id, branchVersion, projectID), data)
}

func publishedLanceStorePath(artType ArtifactType, root string) string {
	switch artType {
	case TypeAST:
		return filepath.Join(root, ast.SearchBundleDir)
	case TypeKnowledge:
		return wiki.WikiIndexPath(root)
	default:
		return ""
	}
}

func publishedLanceStorePart(artType ArtifactType) string {
	switch artType {
	case TypeAST:
		return ast.SearchBundleDir
	case TypeKnowledge:
		return wiki.WikiIndexDirName
	default:
		return ""
	}
}

func lanceTableKeys(name string) ([]string, error) {
	switch name {
	case "files":
		return []string{"path"}, nil
	case "entities":
		return []string{"uid"}, nil
	case "chunks":
		return []string{"slug"}, nil
	case "xrefs":
		return []string{"source_slug", "target_slug"}, nil
	case "sync_log":
		return []string{"timestamp"}, nil
	case "meta":
		return []string{"key"}, nil
	default:
		return nil, fmt.Errorf("no snapshot key is registered for Lance table %q", name)
	}
}

func lanceTableIndexes(name string, rows []lancestore.Row) []lancestore.Index {
	var indexes []lancestore.Index
	switch name {
	case "files":
		indexes = []lancestore.Index{{Column: "source", Kind: lancestore.IndexInvertedText}, {Column: "path", Kind: lancestore.IndexScalarBTree}}
	case "entities":
		indexes = []lancestore.Index{{Column: "body", Kind: lancestore.IndexInvertedText}, {Column: "etype", Kind: lancestore.IndexScalarBitmap}, {Column: "path", Kind: lancestore.IndexScalarBTree}}
	case "chunks":
		indexes = []lancestore.Index{{Column: "body", Kind: lancestore.IndexInvertedText}, {Column: "search_terms", Kind: lancestore.IndexInvertedText}, {Column: "slug", Kind: lancestore.IndexScalarBTree}, {Column: "doc_type", Kind: lancestore.IndexScalarBitmap}, {Column: "superseded", Kind: lancestore.IndexScalarBitmap}, {Column: "mandatory", Kind: lancestore.IndexScalarBitmap}, {Column: "entity_id", Kind: lancestore.IndexScalarBTree}}
	}
	if (name == "entities" || name == "chunks") && embeddedRows(rows) >= 256 {
		indexes = append(indexes, lancestore.Index{Column: "embedding", Kind: lancestore.IndexVectorIVFPQ})
	}
	return indexes
}

func embeddedRows(rows []lancestore.Row) int {
	count := 0
	for _, row := range rows {
		if vector, ok := row["embedding"].([]float32); ok && len(vector) > 0 {
			count++
		}
	}
	return count
}

func (m *RegistryManager) publishBranchLance(ctx context.Context, entryID, branchVersion string, meta *Entry, stagedRoot string, snapshot gitstate.Snapshot) (lanceBranchHistory, error) {
	if snapshot.Branch == "" || snapshot.BranchVersion() != branchVersion {
		return lanceBranchHistory{}, fmt.Errorf("publishing %s requires checked-out branch %q, found %q", branchVersion, strings.TrimPrefix(branchVersion, "branch/"), snapshot.Branch)
	}
	if snapshot.Dirty {
		return lanceBranchHistory{}, errors.New("publishing a branch requires a clean Git worktree so the Lance snapshot maps to exactly one commit")
	}
	localURI := publishedLanceStorePath(meta.Type, stagedRoot)
	remoteURI := m.store.ArtifactURI(meta.Type, entryID, branchVersion, meta.ProjectID, publishedLanceStorePart(meta.Type))
	localConfig := m.store.lanceConfigFor(ctx, localURI, false)
	if sourceURI := lancestore.ShallowSourceURI(localURI); sourceURI != "" {
		localConfig = m.store.lanceConfigFor(ctx, sourceURI, false)
		localConfig.URI = localURI
	}
	local, err := lancestore.Open(ctx, localConfig)
	if err != nil {
		return lanceBranchHistory{}, fmt.Errorf("open local Lance snapshot: %w", err)
	}
	defer func() { _ = local.Close() }()
	remote, err := lancestore.Open(ctx, m.store.lanceConfigFor(ctx, remoteURI, true))
	if err != nil {
		return lanceBranchHistory{}, fmt.Errorf("open Hub Lance branch: %w", err)
	}
	defer func() { _ = remote.Close() }()

	fingerprint := lanceFingerprint(meta.Type)
	history, err := m.store.readBranchHistory(ctx, meta.Type, entryID, branchVersion, meta.ProjectID)
	if err != nil && !errors.Is(err, s3store.ErrNotFound) {
		return lanceBranchHistory{}, err
	}
	if history.Version == 0 {
		history = lanceBranchHistory{Version: branchHistoryVersion, ProjectID: meta.ProjectID, ArtifactType: meta.Type, Branch: snapshot.Branch}
	} else if err := validateLanceHistory(history, meta.Type, meta.ProjectID, snapshot.Branch); err != nil {
		return lanceBranchHistory{}, err
	}
	commit := lanceCommit{Commit: snapshot.Commit, ProducerVersion: version.Version, Fingerprint: fingerprint, PublishedAt: time.Now().UTC(), Tables: map[string]lanceTableRef{}}
	tableNames, err := local.TableNames(ctx)
	if err != nil {
		return lanceBranchHistory{}, err
	}
	sort.Strings(tableNames)
	for _, name := range tableNames {
		source, err := local.OpenTable(ctx, name)
		if err != nil {
			return lanceBranchHistory{}, err
		}
		rows, err := source.Rows(ctx)
		if err != nil {
			return lanceBranchHistory{}, fmt.Errorf("read local table %s: %w", name, err)
		}
		target, err := remote.OpenTable(ctx, name)
		if errors.Is(err, lancestore.ErrNoSuchTable) {
			target, err = remote.CreateTable(ctx, name, source.Schema())
		}
		if err != nil {
			return lanceBranchHistory{}, fmt.Errorf("open Hub table %s: %w", name, err)
		}
		if !target.Schema().Equal(source.Schema()) {
			return lanceBranchHistory{}, fmt.Errorf("hub branch table %s has an incompatible schema; publish a new branch channel after changing embedding compatibility", name)
		}
		keys, err := lanceTableKeys(name)
		if err != nil {
			return lanceBranchHistory{}, err
		}
		changed, err := target.ApplySnapshotDelta(ctx, keys, rows)
		if err != nil {
			return lanceBranchHistory{}, fmt.Errorf("publish table %s delta: %w", name, err)
		}
		indexes := lanceTableIndexes(name, rows)
		if err := target.EnsureIndexes(ctx, indexes...); err != nil {
			return lanceBranchHistory{}, fmt.Errorf("index Hub table %s: %w", name, err)
		}
		if changed > 0 && len(indexes) > 0 {
			if err := target.FoldNewRowsIntoIndexes(ctx); err != nil {
				return lanceBranchHistory{}, fmt.Errorf("update Hub indexes for %s: %w", name, err)
			}
		}
		current, err := target.CurrentVersion(ctx)
		if err != nil {
			return lanceBranchHistory{}, err
		}
		tag := lanceCommitTag(snapshot.Commit)
		if err := target.PutTag(ctx, tag, current); err != nil {
			return lanceBranchHistory{}, err
		}
		commit.Tables[name] = lanceTableRef{Version: current, Tag: tag}
	}

	filtered := history.Commits[:0]
	for _, previous := range history.Commits {
		if previous.Commit != commit.Commit {
			filtered = append(filtered, previous)
		}
	}
	history.Commits = append([]lanceCommit{commit}, filtered...)
	return history, nil
}

func validateLanceHistory(history lanceBranchHistory, artType ArtifactType, projectID, branch string) error {
	if history.ArtifactType != artType || history.ProjectID != projectID || history.Branch != branch {
		return fmt.Errorf("lance branch history belongs to %s project %q branch %q, not %s project %q branch %q", history.ArtifactType, history.ProjectID, history.Branch, artType, projectID, branch)
	}
	return nil
}

// HydrateProjectLance shallow-clones the nearest compatible published ancestor.
func HydrateProjectLance(ctx context.Context, projectDir string, projectCfg config.ConfigMap) error {
	_, err := HydrateProjectLanceWithResult(ctx, projectDir, projectCfg)
	return err
}

// HydrateProjectLanceWithResult checks the remote branch on every invocation.
// When its compatible base changes, a fresh shallow clone replaces the previous
// generated store; the caller then reconciles checkout changes into that base.
func HydrateProjectLanceWithResult(ctx context.Context, projectDir string, projectCfg config.ConfigMap) (HydrationResult, error) {
	var result HydrationResult
	snapshot, err := gitstate.InspectSnapshot(projectDir)
	if err != nil || snapshot.Branch == "" {
		return result, nil
	}
	lock, err := LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lock == nil || lock.Project.ID == "" {
		return result, nil
	}
	s3, err := NewS3Store(ctx, nil, projectCfg)
	if err != nil || !s3.Configured() {
		return result, err
	}
	published, err := authorizeHydrationProject(ctx, s3, lock.Project.ID)
	if err != nil {
		return result, err
	}
	if !published {
		return result, nil
	}
	branchVersion := snapshot.BranchVersion()
	registry := &RegistryManager{store: s3, entries: make(map[ArtifactType]map[string]*Entry)}
	entries, err := registry.ListProjectEntries(ctx, lock.Project.ID)
	if err != nil {
		return result, fmt.Errorf("list published project artifacts for hydration: %w", err)
	}
	targets := []struct {
		artType      ArtifactType
		path         string
		lifecycleDir string
	}{
		{TypeAST, filepath.Join(store.ASTProjectDir(projectDir), ast.SearchBundleDir), store.ASTProjectDir(projectDir)},
		{TypeKnowledge, wiki.WikiIndexPath(store.KnowledgeProjectDir(projectDir)), store.KnowledgeProjectDir(projectDir)},
	}
	for _, target := range targets {
		entryID, entryErr := selectHydrationEntry(entries, lock, target.artType, branchVersion)
		if entryErr != nil {
			return result, entryErr
		}
		if entryID == "" {
			continue
		}
		lockedCtx, lifecycleLock, lockErr := storelifecycle.Acquire(ctx, target.lifecycleDir)
		if lockErr != nil {
			return result, lockErr
		}
		history, readErr := s3.readBranchHistory(lockedCtx, target.artType, entryID, branchVersion, lock.Project.ID)
		if readErr != nil {
			lifecycleLock.Release()
			if errors.Is(readErr, s3store.ErrNotFound) {
				continue
			}
			return result, readErr
		}
		base, ok := selectLanceBase(history, snapshot.Ancestors, lanceFingerprint(target.artType))
		if !ok {
			lifecycleLock.Release()
			continue
		}
		selected := localLanceBase{EntryID: entryID, Branch: branchVersion,
			SourceURI: s3.ArtifactURI(target.artType, entryID, branchVersion, lock.Project.ID, publishedLanceStorePart(target.artType)), Base: base}
		if !sameLocalLanceBase(target.path, selected) {
			if err := hydrateLanceStore(lockedCtx, s3, branchVersion, target.path, base, selected); err != nil {
				lifecycleLock.Release()
				return result, err
			}
		}
		if target.artType == TypeAST {
			result.ASTBaseCommit = base.Commit
		} else {
			result.KnowledgeBaseCommit = base.Commit
		}
		lifecycleLock.Release()
	}
	return result, nil
}

func sameLocalLanceBase(path string, selected localLanceBase) bool {
	if !initializedLanceStore(path) {
		return false
	}
	data, err := os.ReadFile(filepath.Join(path, localBaseFile))
	if err != nil {
		return false
	}
	var current localLanceBase
	if json.Unmarshal(data, &current) != nil {
		return false
	}
	return current.EntryID == selected.EntryID && current.Branch == selected.Branch && current.SourceURI == selected.SourceURI &&
		current.Base.Commit == selected.Base.Commit && current.Base.Fingerprint == selected.Base.Fingerprint &&
		reflect.DeepEqual(current.Base.Tables, selected.Base.Tables)
}

func selectHydrationEntry(entries []*Entry, lock *Lockfile, artType ArtifactType, branchVersion string) (string, error) {
	var candidates []string
	for _, entry := range entries {
		if entry.Type != artType || entry.ProjectID != lock.Project.ID {
			continue
		}
		for _, version := range entry.Versions {
			if version == branchVersion {
				candidates = append(candidates, entry.ID)
				break
			}
		}
	}
	if len(candidates) == 0 {
		return "", nil
	}
	if installed := lock.Artifacts[artType]; installed != nil {
		for name, meta := range installed {
			if meta == nil || meta.Version != branchVersion || (meta.ProjectID != "" && meta.ProjectID != lock.Project.ID) {
				continue
			}
			id := meta.RemoteID
			if id == "" {
				id = name
			}
			for _, candidate := range candidates {
				if candidate == id {
					return candidate, nil
				}
			}
		}
	}
	if len(candidates) == 1 {
		return candidates[0], nil
	}
	sort.Strings(candidates)
	return "", fmt.Errorf("multiple %s artifacts publish %s for project %s (%s); pin one in the project lockfile", artType, branchVersion, lock.Project.ID, strings.Join(candidates, ", "))
}

func authorizeHydrationProject(ctx context.Context, s3 *S3Store, projectID string) (bool, error) {
	err := s3.AuthorizeProject(ctx, projectID)
	if errors.Is(err, s3store.ErrNotFound) {
		return false, nil
	}
	return err == nil, err
}

func initializedLanceStore(path string) bool {
	entries, err := os.ReadDir(path)
	if err != nil {
		return false
	}
	for _, entry := range entries {
		if entry.IsDir() && strings.HasSuffix(entry.Name(), ".lance") {
			return true
		}
	}
	return false
}

func hydrateLanceStore(ctx context.Context, s3 *S3Store, branchVersion, targetPath string, base lanceCommit, selected localLanceBase) error {
	if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
		return err
	}
	staging, err := os.MkdirTemp(filepath.Dir(targetPath), filepath.Base(targetPath)+".hydrate-")
	if err != nil {
		return err
	}
	defer func() { _ = os.RemoveAll(staging) }()
	remoteStore := selected.SourceURI
	localConfig := s3.lanceConfigFor(ctx, remoteStore, true)
	localConfig.URI = staging
	local, err := lancestore.Open(ctx, localConfig)
	if err != nil {
		return err
	}
	tableNames := make([]string, 0, len(base.Tables))
	for name := range base.Tables {
		tableNames = append(tableNames, name)
	}
	sort.Strings(tableNames)
	for _, name := range tableNames {
		ref := base.Tables[name]
		sourceURI := strings.TrimSuffix(remoteStore, "/") + "/" + name + ".lance"
		if _, err := local.CloneTable(ctx, name, sourceURI, lancestore.CloneOptions{SourceTag: ref.Tag}); err != nil {
			_ = local.Close()
			return fmt.Errorf("hydrate %s from %s at %s: %w", name, branchVersion, base.Commit, err)
		}
	}
	if err := local.Close(); err != nil {
		return err
	}
	marker, err := json.Marshal(selected)
	if err != nil {
		return err
	}
	if err := os.WriteFile(filepath.Join(staging, localBaseFile), marker, 0o600); err != nil {
		return err
	}
	backup := staging + ".previous"
	if _, err := os.Stat(targetPath); err == nil {
		if err := os.Rename(targetPath, backup); err != nil {
			return err
		}
	} else if !os.IsNotExist(err) {
		return err
	}
	if err := os.Rename(staging, targetPath); err != nil {
		if _, statErr := os.Stat(backup); statErr == nil {
			_ = os.Rename(backup, targetPath)
		}
		return err
	}
	return os.RemoveAll(backup)
}
