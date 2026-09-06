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
	"sort"
	"strings"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

const (
	branchHistoryFile    = "graphit-history.json"
	branchHistoryVersion = 1
)

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
	local, err := lancestore.Open(ctx, m.store.lanceConfig(localURI, false))
	if err != nil {
		return lanceBranchHistory{}, fmt.Errorf("open local Lance snapshot: %w", err)
	}
	defer func() { _ = local.Close() }()
	remote, err := lancestore.Open(ctx, m.store.lanceConfig(remoteURI, true))
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
		if _, err := target.ReplaceSnapshot(ctx, keys, rows); err != nil {
			return lanceBranchHistory{}, fmt.Errorf("publish table %s: %w", name, err)
		}
		indexes := lanceTableIndexes(name, rows)
		if err := target.EnsureIndexes(ctx, indexes...); err != nil {
			return lanceBranchHistory{}, fmt.Errorf("index Hub table %s: %w", name, err)
		}
		if len(indexes) > 0 {
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

// HydrateProjectLance shallow-clones the nearest compatible published ancestor into empty local stores.
func HydrateProjectLance(ctx context.Context, projectDir string, projectCfg config.ConfigMap) error {
	snapshot, err := gitstate.InspectSnapshot(projectDir)
	if err != nil || snapshot.Branch == "" {
		return nil
	}
	lock, err := LoadLockfile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil || lock == nil || lock.Project.ID == "" {
		return nil
	}
	s3, err := NewS3Store(ctx, nil, projectCfg)
	if err != nil || !s3.Configured() {
		return err
	}
	if err := hubaccess.AuthorizeProject(ctx, s3, lock.Project.ID); err != nil {
		return err
	}
	branchVersion := snapshot.BranchVersion()
	targets := []struct {
		artType ArtifactType
		path    string
	}{
		{TypeAST, filepath.Join(store.ASTProjectDir(projectDir), ast.SearchBundleDir)},
		{TypeKnowledge, wiki.WikiIndexPath(store.KnowledgeProjectDir(projectDir))},
	}
	for _, target := range targets {
		if initializedLanceStore(target.path) {
			continue
		}
		history, readErr := s3.readBranchHistory(ctx, target.artType, "", branchVersion, lock.Project.ID)
		if readErr != nil {
			if errors.Is(readErr, s3store.ErrNotFound) {
				continue
			}
			return readErr
		}
		base, ok := selectLanceBase(history, snapshot.Ancestors, lanceFingerprint(target.artType))
		if !ok {
			continue
		}
		if err := hydrateLanceStore(ctx, s3, target.artType, branchVersion, lock.Project.ID, target.path, base); err != nil {
			return err
		}
	}
	return nil
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

func hydrateLanceStore(ctx context.Context, s3 *S3Store, artType ArtifactType, branchVersion, projectID, targetPath string, base lanceCommit) error {
	if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
		return err
	}
	staging, err := os.MkdirTemp(filepath.Dir(targetPath), filepath.Base(targetPath)+".hydrate-")
	if err != nil {
		return err
	}
	defer func() { _ = os.RemoveAll(staging) }()
	local, err := lancestore.Open(ctx, s3.lanceConfig(staging, true))
	if err != nil {
		return err
	}
	tableNames := make([]string, 0, len(base.Tables))
	for name := range base.Tables {
		tableNames = append(tableNames, name)
	}
	sort.Strings(tableNames)
	remoteStore := s3.ArtifactURI(artType, "", branchVersion, projectID, publishedLanceStorePart(artType))
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
	if initializedLanceStore(targetPath) {
		return nil
	}
	if err := os.RemoveAll(targetPath); err != nil {
		return err
	}
	return os.Rename(staging, targetPath)
}
