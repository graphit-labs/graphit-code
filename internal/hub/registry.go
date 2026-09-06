package hub

import (
	"context"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	gitstate "github.com/graphit-labs/graphit-code/internal/git"
	"github.com/graphit-labs/graphit-code/internal/hubaccess"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	paths_pkg "github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/s3store"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

var TypeFolderMap = map[ArtifactType]string{
	TypeAgent:     "agents",
	TypeRule:      "rules",
	TypeWorkflow:  "workflows",
	TypeSkill:     "skills",
	TypeKnowledge: "knowledge",
	TypeAST:       "ast",
	TypeMCP:       "mcp-servers",
	TypeCommand:   "commands",
	TypePower:     "powers",
	TypeLanguage:  "languages",
}

var ValidTypes = []ArtifactType{
	TypeAgent, TypeRule, TypeWorkflow, TypeSkill,
	TypeKnowledge, TypeAST, TypeMCP, TypeCommand, TypePower,
	TypeLanguage,
}

type Entry struct {
	ID           string            `json:"id"`
	Name         string            `json:"name"`
	Type         ArtifactType      `json:"type"`
	Description  string            `json:"description,omitempty"`
	Tags         []string          `json:"tags,omitempty"`
	Author       *Author           `json:"author,omitempty"`
	Latest       string            `json:"latest,omitempty"`
	Versions     []string          `json:"versions,omitempty"`
	Hashes       map[string]string `json:"hashes,omitempty"`
	Dependencies []Dependency      `json:"dependencies,omitempty"`
	ProjectID    string            `json:"project_id,omitempty"`
}

type Author struct {
	Username string `json:"username,omitempty"`
}

type Dependency struct {
	ID      string       `json:"id"`
	Type    ArtifactType `json:"type,omitempty"`
	Version string       `json:"version,omitempty"`
}

type Project struct {
	ID          string `json:"id"`
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
	Revision    int64  `json:"revision"`
	Status      string `json:"status"`
}

type Baseline struct {
	ID      string       `json:"id"`
	Type    ArtifactType `json:"type"`
	Version string       `json:"version"`
}

type projectFile struct {
	Version int      `json:"v"`
	Project *Project `json:"project,omitempty"`
}

type entryFile struct {
	Version int   `json:"v"`
	Entry   Entry `json:"entry"`
}

type baselinesData struct {
	Baselines []Baseline `json:"baselines"`
}

type nameRecord struct {
	Version         int    `json:"v"`
	Name            string `json:"name"`
	ProjectID       string `json:"project_id"`
	ProjectRevision int64  `json:"project_revision"`
	Status          string `json:"status"`
}

type discoveryCursor struct {
	Scope int    `json:"scope"`
	Exact int    `json:"exact,omitempty"`
	S3    string `json:"s3,omitempty"`
}

type entryCursor struct {
	Project  string `json:"project,omitempty"`
	Artifact string `json:"artifact,omitempty"`
}

type ProjectPage struct {
	Projects   []*Project `json:"projects"`
	NextCursor string     `json:"next_cursor,omitempty"`
}

type EntryPage struct {
	Entries    []*Entry `json:"entries"`
	NextCursor string   `json:"next_cursor,omitempty"`
}

const hubManifestVersion = 2

type RegistryManager struct {
	Logger        *slog.Logger
	store         *S3Store
	projectConfig config.ConfigMap

	baseCtx context.Context
	cache   *metadataCache
	dataMu  sync.RWMutex

	entries  map[ArtifactType]map[string]*Entry
	projects map[string]*Project
}

func (r *RegistryManager) log() *slog.Logger { return slogutil.Resolve(r.Logger) }

func NewRegistryManager(ctx context.Context) (*RegistryManager, error) {
	m := &RegistryManager{
		entries:  make(map[ArtifactType]map[string]*Entry),
		projects: make(map[string]*Project),
		baseCtx:  ctx,
	}

	var projectCfg config.ConfigMap
	pp := paths_pkg.GetPaths("", false)
	if lf, err := LoadLockfile(pp.LockFilePath); err == nil && lf != nil {
		projectCfg = lf.Config
	}
	m.projectConfig = projectCfg

	st, err := NewS3Store(ctx, nil, projectCfg)
	if err == nil && st.Configured() {
		m.store = st
		if subject, subjectErr := hubaccess.TrustedSubject(ctx); subjectErr == nil {
			m.cache = newMetadataCache(st.CacheDir(), st.cfg, subject)
		}
	}

	return m, nil
}

func (m *RegistryManager) DiscoverProjects(ctx context.Context, limit int, cursor string) (ProjectPage, error) {
	if m.store == nil {
		return ProjectPage{}, fmt.Errorf("hub not configured")
	}
	grants, _, err := hubaccess.ResolveTrusted(ctx, m.store)
	if err != nil {
		return ProjectPage{}, err
	}
	if limit <= 0 || limit > 1000 {
		limit = 100
	}
	state, err := decodeDiscoveryCursor(cursor)
	if err != nil {
		return ProjectPage{}, err
	}

	result := ProjectPage{}
	seen := make(map[string]struct{})
	exactIDs := grants.ExactIDs()
	exactSet := make(map[string]struct{}, len(exactIDs))
	for _, projectID := range exactIDs {
		exactSet[projectID] = struct{}{}
	}
	if state.Scope == 0 {
		if state.Exact > len(exactIDs) {
			return ProjectPage{}, fmt.Errorf("invalid discovery cursor")
		}
		for index, projectID := range exactIDs[state.Exact:] {
			project, err := m.readProjectCached(ctx, projectID)
			if errors.Is(err, s3store.ErrNotFound) {
				continue
			}
			if err != nil {
				return ProjectPage{}, err
			}
			m.acceptProject(project)
			seen[project.ID] = struct{}{}
			result.Projects = append(result.Projects, project)
			if len(result.Projects) == limit {
				next := state.Exact + index + 1
				if next < len(exactIDs) {
					result.NextCursor = encodeDiscoveryCursor(discoveryCursor{Exact: next})
				} else {
					result.NextCursor = encodeDiscoveryCursor(discoveryCursor{Scope: 1})
				}
				return result, nil
			}
		}
		state.Scope = 1
	}

	scopes := grants.NamePrefixes()
	if grants.All() {
		scopes = []string{""}
	}
	for scope := state.Scope - 1; scope < len(scopes); scope++ {
		prefix := hubaccess.NameDirectoryPrefix() + scopes[scope]
		page, err := m.store.ListPage(ctx, prefix, limit-len(result.Projects), state.S3)
		if err != nil {
			return ProjectPage{}, err
		}
		for _, object := range page.Objects {
			project, err := m.readNameProject(ctx, object.Key)
			if errors.Is(err, s3store.ErrNotFound) {
				continue
			}
			if err != nil {
				return ProjectPage{}, err
			}
			if !grants.Allows(project.ID, project.Name) {
				continue
			}
			if _, exact := exactSet[project.ID]; exact {
				continue
			}
			if _, duplicate := seen[project.ID]; duplicate {
				continue
			}
			seen[project.ID] = struct{}{}
			m.acceptProject(project)
			result.Projects = append(result.Projects, project)
		}
		if page.NextCursor != "" {
			result.NextCursor = encodeDiscoveryCursor(discoveryCursor{Scope: scope + 1, S3: page.NextCursor})
			return result, nil
		}
		state.S3 = ""
		if len(result.Projects) == limit && scope+1 < len(scopes) {
			result.NextCursor = encodeDiscoveryCursor(discoveryCursor{Scope: scope + 2})
			return result, nil
		}
	}
	return result, nil
}

func (m *RegistryManager) readProject(ctx context.Context, projectID string) (*Project, error) {
	if err := hubaccess.ValidateProjectID(projectID); err != nil {
		return nil, err
	}
	value, err := m.store.ReadValue(ctx, hubaccess.ProjectMetadataKey(projectID))
	if err != nil {
		return nil, err
	}
	project, err := decodeProject(projectID, value.Data)
	if err == nil && m.cache != nil {
		_ = m.cache.Put(hubaccess.ProjectMetadataKey(projectID), value.Data, value.ETag)
	}
	return project, err
}

func (m *RegistryManager) readProjectCached(ctx context.Context, projectID string) (*Project, error) {
	if err := hubaccess.ValidateProjectID(projectID); err != nil {
		return nil, err
	}
	key := hubaccess.ProjectMetadataKey(projectID)
	if m.cache != nil {
		if data, _, ok := m.cache.GetFresh(key); ok {
			if project, err := decodeProject(projectID, data); err == nil {
				return project, nil
			}
		}
	}
	return m.readProject(ctx, projectID)
}

func decodeProject(projectID string, data []byte) (*Project, error) {
	var file projectFile
	if err := json.Unmarshal(data, &file); err != nil {
		return nil, err
	}
	if file.Version != hubManifestVersion || file.Project == nil || file.Project.ID != projectID || file.Project.Status != "active" {
		return nil, fmt.Errorf("invalid project metadata for %s", projectID)
	}
	name, err := hubaccess.NormalizeProjectName(file.Project.Name)
	if err != nil || name != file.Project.Name {
		return nil, fmt.Errorf("invalid project metadata name for %s", projectID)
	}
	return file.Project, nil
}

func (m *RegistryManager) readNameProject(ctx context.Context, key string) (*Project, error) {
	var record nameRecord
	if err := ReadJSON(ctx, m.store, key, &record); err != nil {
		return nil, err
	}
	if record.Version != hubManifestVersion || record.Status != "active" {
		return nil, s3store.ErrNotFound
	}
	if hubaccess.NameRecordKey(record.Name) != key {
		return nil, fmt.Errorf("name directory key %s disagrees with record %q", key, record.Name)
	}
	project, err := m.readProject(ctx, record.ProjectID)
	if err != nil {
		return nil, err
	}
	if project.Name != record.Name || project.Revision != record.ProjectRevision {
		return nil, fmt.Errorf("name directory record %s disagrees with project %s", key, project.ID)
	}
	return project, nil
}

func (m *RegistryManager) acceptProject(project *Project) {
	m.dataMu.Lock()
	defer m.dataMu.Unlock()
	if m.projects == nil {
		m.projects = make(map[string]*Project)
	}
	m.projects[project.ID] = project
}

func encodeDiscoveryCursor(cursor discoveryCursor) string {
	data, _ := json.Marshal(cursor)
	return base64.RawURLEncoding.EncodeToString(data)
}

func decodeDiscoveryCursor(value string) (discoveryCursor, error) {
	if value == "" {
		return discoveryCursor{}, nil
	}
	data, err := base64.RawURLEncoding.DecodeString(value)
	if err != nil {
		return discoveryCursor{}, fmt.Errorf("invalid discovery cursor")
	}
	var cursor discoveryCursor
	if json.Unmarshal(data, &cursor) != nil || cursor.Scope < 0 {
		return discoveryCursor{}, fmt.Errorf("invalid discovery cursor")
	}
	return cursor, nil
}

func (m *RegistryManager) GetEntry(id string, entryType ArtifactType) *Entry {
	m.dataMu.RLock()
	defer m.dataMu.RUnlock()
	if entryType != "" {
		return uniqueEntry(m.entries[entryType], id)
	}
	var found *Entry
	for _, typeMap := range m.entries {
		if entry := uniqueEntry(typeMap, id); entry != nil {
			if found != nil {
				return nil
			}
			found = entry
		}
	}
	return found
}

func (m *RegistryManager) GetEntryInProject(projectID, id string, entryType ArtifactType) *Entry {
	m.dataMu.RLock()
	defer m.dataMu.RUnlock()
	if entryType != "" {
		return m.entries[entryType][entryKey(projectID, id)]
	}
	var found *Entry
	for _, typeMap := range m.entries {
		if entry := typeMap[entryKey(projectID, id)]; entry != nil {
			if found != nil {
				return nil
			}
			found = entry
		}
	}
	return found
}

func (m *RegistryManager) ResolveEntry(ctx context.Context, projectID, id string, entryType ArtifactType) (*Entry, error) {
	if projectID == "" {
		if m.store == nil {
			entry := m.GetEntry(id, entryType)
			if entry == nil {
				return nil, fmt.Errorf("artifact %q is absent or ambiguous; provide project_id", id)
			}
			return entry, nil
		}
		var found *Entry
		cursor := ""
		for {
			page, err := m.ListEntriesPage(ctx, entryType, 100, cursor)
			if err != nil {
				return nil, err
			}
			for _, entry := range page.Entries {
				if entry.ID != id {
					continue
				}
				if found != nil && found.ProjectID != entry.ProjectID {
					return nil, fmt.Errorf("artifact %q is ambiguous; provide project_id", id)
				}
				found = entry
			}
			if page.NextCursor == "" {
				break
			}
			cursor = page.NextCursor
		}
		if found == nil {
			return nil, fmt.Errorf("artifact %q was not found in accessible projects", id)
		}
		return found, nil
	}
	if m.store == nil {
		entry := m.GetEntryInProject(projectID, id, entryType)
		if entry == nil {
			return nil, fmt.Errorf("artifact %q was not found in project %s", id, projectID)
		}
		return entry, nil
	}
	if err := m.authorizeProject(ctx, projectID); err != nil {
		return nil, err
	}
	if entryType == "" {
		return nil, fmt.Errorf("artifact type is required with project_id")
	}
	key := hubaccess.ProjectRegistryKey(projectID, string(entryType), id)
	var file entryFile
	if err := ReadJSON(ctx, m.store, key, &file); err != nil {
		return nil, err
	}
	if file.Version != hubManifestVersion || file.Entry.ProjectID != projectID || file.Entry.ID != id || file.Entry.Type != entryType {
		return nil, fmt.Errorf("invalid registry entry %s", key)
	}
	entry := file.Entry
	return &entry, nil
}

func uniqueEntry(entries map[string]*Entry, id string) *Entry {
	var found *Entry
	for _, entry := range entries {
		if entry.ID != id {
			continue
		}
		if found != nil {
			return nil
		}
		found = entry
	}
	return found
}

func entryKey(projectID, id string) string {
	return projectID + "\x00" + id
}

func encodeEntryCursor(cursor entryCursor) string {
	data, _ := json.Marshal(cursor)
	return base64.RawURLEncoding.EncodeToString(data)
}

func decodeEntryCursor(value string) (entryCursor, error) {
	if value == "" {
		return entryCursor{}, nil
	}
	data, err := base64.RawURLEncoding.DecodeString(value)
	if err != nil {
		return entryCursor{}, fmt.Errorf("invalid entry cursor: %w", err)
	}
	var cursor entryCursor
	if err := json.Unmarshal(data, &cursor); err != nil {
		return entryCursor{}, fmt.Errorf("invalid entry cursor: %w", err)
	}
	return cursor, nil
}

func (m *RegistryManager) loadEntryPage(ctx context.Context, projectID string, limit int, cursor string) ([]*Entry, string, error) {
	page, err := m.store.ListPage(ctx, hubaccess.ProjectRegistryPrefix(projectID), limit, cursor)
	if err != nil {
		return nil, "", err
	}
	entries := make([]*Entry, 0, len(page.Objects))
	for _, object := range page.Objects {
		var file entryFile
		if err := ReadJSON(ctx, m.store, object.Key, &file); err != nil {
			return nil, "", err
		}
		entry := file.Entry
		if file.Version != hubManifestVersion || entry.ProjectID != projectID || entry.ID == "" || entry.Type == "" {
			return nil, "", fmt.Errorf("invalid registry entry %s", object.Key)
		}
		entryCopy := entry
		m.dataMu.Lock()
		if m.entries[entry.Type] == nil {
			m.entries[entry.Type] = make(map[string]*Entry)
		}
		m.entries[entry.Type][entryKey(projectID, entry.ID)] = &entryCopy
		m.dataMu.Unlock()
		entries = append(entries, &entryCopy)
	}
	return entries, page.NextCursor, nil
}

func (m *RegistryManager) ListEntries(typeFilter ArtifactType) []*Entry {
	m.dataMu.RLock()
	defer m.dataMu.RUnlock()
	var result []*Entry
	for t, typeMap := range m.entries {
		if typeFilter != "" && t != typeFilter {
			continue
		}
		for _, e := range typeMap {
			result = append(result, e)
		}
	}
	return result
}

func (m *RegistryManager) ListEntriesPage(ctx context.Context, typeFilter ArtifactType, limit int, cursor string) (EntryPage, error) {
	if limit <= 0 || limit > 1000 {
		limit = 100
	}
	state, err := decodeEntryCursor(cursor)
	if err != nil {
		return EntryPage{}, err
	}

	result := EntryPage{}
	for scannedProjects := 0; len(result.Entries) < limit && scannedProjects < limit; scannedProjects++ {
		projects, err := m.DiscoverProjects(ctx, 1, state.Project)
		if err != nil {
			return EntryPage{}, err
		}
		if len(projects.Projects) == 0 {
			return result, nil
		}
		project := projects.Projects[0]
		entries, nextArtifact, err := m.loadEntryPage(ctx, project.ID, limit-len(result.Entries), state.Artifact)
		if err != nil {
			return EntryPage{}, err
		}
		for _, entry := range entries {
			if typeFilter == "" || entry.Type == typeFilter {
				result.Entries = append(result.Entries, entry)
			}
		}
		if nextArtifact != "" {
			result.NextCursor = encodeEntryCursor(entryCursor{Project: state.Project, Artifact: nextArtifact})
			return result, nil
		}
		state.Project = projects.NextCursor
		state.Artifact = ""
		if state.Project == "" {
			return result, nil
		}
	}
	result.NextCursor = encodeEntryCursor(state)
	return result, nil
}

func (m *RegistryManager) SearchEntriesPage(ctx context.Context, term string, typeFilter ArtifactType, limit int, cursor string) (EntryPage, error) {
	page, err := m.ListEntriesPage(ctx, typeFilter, limit, cursor)
	if err != nil {
		return EntryPage{}, err
	}
	lower := strings.ToLower(term)
	filtered := page.Entries[:0]
	for _, entry := range page.Entries {
		if strings.Contains(strings.ToLower(entry.Name), lower) || strings.Contains(strings.ToLower(entry.ID), lower) || strings.Contains(strings.ToLower(entry.Description), lower) {
			filtered = append(filtered, entry)
		}
	}
	page.Entries = filtered
	return page, nil
}

func (m *RegistryManager) ListProjectEntries(ctx context.Context, projectID string) ([]*Entry, error) {
	if m.store == nil {
		return m.ListEntries(""), nil
	}
	if err := m.authorizeProject(ctx, projectID); err != nil {
		return nil, err
	}
	var result []*Entry
	cursor := ""
	for {
		entries, next, err := m.loadEntryPage(ctx, projectID, 100, cursor)
		if err != nil {
			return nil, err
		}
		result = append(result, entries...)
		if next == "" {
			return result, nil
		}
		cursor = next
	}
}

func (m *RegistryManager) ResolveProject(ctx context.Context, idOrName string) (*Project, error) {
	if m.store == nil {
		m.dataMu.RLock()
		defer m.dataMu.RUnlock()
		for _, project := range m.projects {
			if project.ID == idOrName || project.Name == idOrName {
				return project, nil
			}
		}
		return nil, fmt.Errorf("project %q was not found", idOrName)
	}
	if hubaccess.ValidateProjectID(idOrName) == nil {
		project, err := m.readProject(ctx, idOrName)
		if err != nil {
			return nil, err
		}
		if err := hubaccess.Authorize(ctx, m.store, project.ID, project.Name); err != nil {
			return nil, err
		}
		m.acceptProject(project)
		return project, nil
	}
	name, err := hubaccess.NormalizeProjectName(idOrName)
	if err != nil {
		return nil, err
	}
	project, err := m.readNameProject(ctx, hubaccess.NameRecordKey(name))
	if err != nil {
		return nil, err
	}
	if err := hubaccess.Authorize(ctx, m.store, project.ID, project.Name); err != nil {
		return nil, err
	}
	m.acceptProject(project)
	return project, nil
}

func (m *RegistryManager) GetDefaultBaselines(_ context.Context) ([]Baseline, error) {
	if m.store == nil {
		return nil, fmt.Errorf("hub not configured")
	}
	data, err := m.store.ReadFile(m.baseCtx, hubaccess.BaselinesKey())
	if err != nil {
		return nil, err
	}
	var bd baselinesData
	if err := json.Unmarshal(data, &bd); err != nil {
		return nil, err
	}
	return bd.Baselines, nil
}

func (m *RegistryManager) PublishEntry(ctx context.Context, entryID string, localPath string, meta *Entry, version string) error {
	if m.store == nil {
		return fmt.Errorf("hub not configured — run '%s setup' first", brand.BinName())
	}
	if err := ValidatePublishedVersion(version); err != nil {
		return fmt.Errorf("invalid published version: %w", err)
	}
	if err := m.authorizeProject(ctx, meta.ProjectID); err != nil {
		return err
	}

	publishPath := localPath
	latestOnly := publicationKeepsLatestOnly(version)
	branchLanceRequested := IsMountable(meta.Type) && strings.HasPrefix(version, "branch/")
	branchLance := branchLanceRequested
	var snapshot gitstate.Snapshot
	if branchLance {
		var err error
		snapshot, err = gitstate.InspectSnapshot(localPath)
		if errors.Is(err, gitstate.ErrNotRepository) {
			branchLance = false
		} else if err != nil {
			return fmt.Errorf("resolve publishing Git snapshot: %w", err)
		}
	}
	if branchLanceRequested && !branchLance {
		_, err := m.store.readBranchHistory(ctx, meta.Type, entryID, version, meta.ProjectID)
		if err == nil {
			return fmt.Errorf("cannot publish non-Git snapshot over Git-backed Lance branch %q; use a different branch name or publish from its Git repository", version)
		}
		if !errors.Is(err, s3store.ErrNotFound) {
			return fmt.Errorf("checking existing Lance branch history: %w", err)
		}
	}

	switch meta.Type {
	case TypeAST:
		storageURI := m.store.ArtifactURI(TypeAST, entryID, version, meta.ProjectID, ast.IcebugBundleDir)
		if storageURI == "" {
			return fmt.Errorf("preparing AST publish: the hub is not configured, so there is no " +
				"location to point the published graph at")
		}
		prepared, err := prepareASTPublishVersion(ctx, localPath, storageURI, m.projectConfig, m.Logger, latestOnly, m.store.cfg)
		if err != nil {
			return fmt.Errorf("preparing AST publish: %w", err)
		}
		defer func() { _ = os.RemoveAll(prepared) }()
		publishPath = prepared
	case TypeKnowledge:
		prepared, err := prepareKnowledgePublishVersion(ctx, localPath, latestOnly, m.store.cfg)
		if err != nil {
			return fmt.Errorf("preparing knowledge publish: %w", err)
		}
		defer func() { _ = os.RemoveAll(prepared) }()
		publishPath = prepared
	}
	versionHash, err := HashDirectory(publishPath)
	if err != nil {
		return fmt.Errorf("computing artifact hash: %w", err)
	}

	if branchLance {
		history, err := m.publishBranchLance(ctx, entryID, version, meta, publishPath, snapshot)
		if err != nil {
			return fmt.Errorf("publishing Lance branch: %w", err)
		}
		if err := m.store.PublishBranchFiles(ctx, meta.Type, entryID, version, meta.ProjectID, publishPath); err != nil {
			return fmt.Errorf("publishing branch files: %w", err)
		}
		if err := m.store.writeBranchHistory(ctx, meta.Type, entryID, version, meta.ProjectID, history); err != nil {
			return fmt.Errorf("recording Lance branch history: %w", err)
		}
	} else if err := m.store.PublishArtifact(ctx, meta.Type, entryID, version, meta.ProjectID, publishPath); err != nil {
		return fmt.Errorf("publishing artifact: %w", err)
	}

	existing := &Entry{}
	var existingFile entryFile
	entryPath := hubaccess.ProjectRegistryKey(meta.ProjectID, string(meta.Type), entryID)
	if err := ReadJSON(ctx, m.store, entryPath, &existingFile); err == nil {
		if existingFile.Version != hubManifestVersion || existingFile.Entry.ProjectID != meta.ProjectID {
			return fmt.Errorf("invalid registry entry %s", entryPath)
		}
		existing = &existingFile.Entry
	} else if !errors.Is(err, s3store.ErrNotFound) {
		return err
	}

	meta.ID = entryID
	if meta.Name == "" {
		meta.Name = existing.Name
		if meta.Name == "" {
			meta.Name = entryID
		}
	}

	versions := existing.Versions
	hasVersion := false
	for _, v := range versions {
		if v == version {
			hasVersion = true
			break
		}
	}
	if !hasVersion {
		versions = append(versions, version)
	}
	meta.Versions = versions
	meta.Latest = version

	hashes := make(map[string]string)
	for k, v := range existing.Hashes {
		hashes[k] = v
	}
	hashes[version] = versionHash
	meta.Hashes = hashes

	m.dataMu.Lock()
	if m.entries[meta.Type] == nil {
		m.entries[meta.Type] = make(map[string]*Entry)
	}
	m.entries[meta.Type][entryKey(meta.ProjectID, entryID)] = meta
	m.dataMu.Unlock()

	if err := m.persistEntryFile(meta); err != nil {
		return err
	}
	return nil
}

func publicationKeepsLatestOnly(version string) bool {
	return strings.HasPrefix(version, "tag/")
}

func (m *RegistryManager) DeleteEntry(ctx context.Context, entryID string, entryType ArtifactType) error {
	if m.store == nil {
		return fmt.Errorf("hub not configured")
	}

	entry := m.GetEntry(entryID, entryType)
	if entry == nil {
		return fmt.Errorf("entry %q not found in registry", entryID)
	}
	if err := m.authorizeProject(ctx, entry.ProjectID); err != nil {
		return err
	}

	for _, ver := range entry.Versions {
		if err := m.store.DeleteArtifact(ctx, entry.Type, entryID, ver, entry.ProjectID); err != nil {
			m.log().Warn("delete branch", "id", entryID, "version", ver, "error", err)
		}
	}

	entryRelPath := hubaccess.ProjectRegistryKey(entry.ProjectID, string(entry.Type), entry.ID)
	if err := m.store.RemoveFile(ctx, entryRelPath); err != nil {
		m.log().Warn("remove entry file", "path", entryRelPath, "error", err)
	}

	m.dataMu.Lock()
	delete(m.entries[entry.Type], entryKey(entry.ProjectID, entryID))
	if len(m.entries[entry.Type]) == 0 {
		delete(m.entries, entry.Type)
	}
	m.dataMu.Unlock()

	return nil
}

func (m *RegistryManager) UpsertProject(ctx context.Context, remoteID, name, description string) (*Project, error) {
	if err := hubaccess.ValidateProjectID(remoteID); err != nil {
		return nil, err
	}
	name, err := hubaccess.NormalizeProjectName(name)
	if err != nil {
		return nil, err
	}
	if m.store == nil {
		return nil, fmt.Errorf("hub not configured")
	}
	grants, _, err := hubaccess.ResolveTrusted(ctx, m.store)
	if err != nil {
		return nil, err
	}
	if !grants.Allows(remoteID, name) {
		return nil, fmt.Errorf("%w: %s", hubaccess.ErrDenied, remoteID)
	}
	return m.upsertProject(ctx, remoteID, name, description)
}

func (m *RegistryManager) authorizeProject(ctx context.Context, projectID string) error {
	project, err := m.readProject(ctx, projectID)
	if err != nil {
		return err
	}
	return hubaccess.Authorize(ctx, m.store, project.ID, project.Name)
}

func (m *RegistryManager) upsertProject(ctx context.Context, projectID, name, description string) (*Project, error) {
	projectKey := hubaccess.ProjectMetadataKey(projectID)
	currentValue, err := m.store.ReadValue(ctx, projectKey)
	if errors.Is(err, s3store.ErrNotFound) {
		return m.createProject(ctx, projectID, name, description)
	}
	if err != nil {
		return nil, err
	}
	var currentFile projectFile
	if err := json.Unmarshal(currentValue.Data, &currentFile); err != nil || currentFile.Version != hubManifestVersion || currentFile.Project == nil || currentFile.Project.ID != projectID {
		return nil, fmt.Errorf("invalid project metadata for %s", projectID)
	}
	current := currentFile.Project
	if current.Status != "active" {
		return nil, fmt.Errorf("project %s is not active", projectID)
	}

	next := &Project{ID: projectID, Name: name, Description: description, Revision: current.Revision + 1, Status: "active"}
	if current.Name == name {
		if err := m.writeProjectCAS(ctx, projectKey, currentValue.ETag, next); err != nil {
			return nil, err
		}
		if err := m.refreshNameRecord(ctx, name, next); err != nil {
			return nil, err
		}
		m.acceptProject(next)
		return next, nil
	}

	reservationETag, err := m.reserveName(ctx, name, next)
	if err != nil {
		return nil, err
	}
	if err := m.writeProjectCAS(ctx, projectKey, currentValue.ETag, next); err != nil {
		_ = m.store.RemoveFileIfMatch(ctx, hubaccess.NameRecordKey(name), reservationETag)
		return nil, err
	}
	if err := m.activateReservedName(ctx, name, reservationETag, next); err != nil {
		return nil, err
	}
	if err := m.removeOwnedName(ctx, current.Name, projectID); err != nil {
		return nil, err
	}
	m.acceptProject(next)
	return next, nil
}

func (m *RegistryManager) createProject(ctx context.Context, projectID, name, description string) (*Project, error) {
	project := &Project{ID: projectID, Name: name, Description: description, Revision: 1, Status: "active"}
	reservationETag, err := m.reserveName(ctx, name, project)
	if err != nil {
		return nil, err
	}
	data, err := json.MarshalIndent(projectFile{Version: hubManifestVersion, Project: project}, "", "  ")
	if err != nil {
		return nil, err
	}
	if _, err := m.store.WriteFileIfAbsent(ctx, hubaccess.ProjectMetadataKey(projectID), data); err != nil {
		_ = m.store.RemoveFileIfMatch(ctx, hubaccess.NameRecordKey(name), reservationETag)
		return nil, fmt.Errorf("creating project %s: %w", projectID, err)
	}
	if err := m.activateReservedName(ctx, name, reservationETag, project); err != nil {
		return nil, err
	}
	m.acceptProject(project)
	return project, nil
}

func (m *RegistryManager) reserveName(ctx context.Context, name string, project *Project) (string, error) {
	record := nameRecord{Version: hubManifestVersion, Name: name, ProjectID: project.ID, ProjectRevision: project.Revision, Status: "pending"}
	data, err := json.MarshalIndent(record, "", "  ")
	if err != nil {
		return "", err
	}
	etag, err := m.store.WriteFileIfAbsent(ctx, hubaccess.NameRecordKey(name), data)
	if errors.Is(err, s3store.ErrPreconditionFailed) {
		return "", fmt.Errorf("project name %q is already reserved", name)
	}
	return etag, err
}

func (m *RegistryManager) activateReservedName(ctx context.Context, name, etag string, project *Project) error {
	record := nameRecord{Version: hubManifestVersion, Name: name, ProjectID: project.ID, ProjectRevision: project.Revision, Status: "active"}
	data, err := json.MarshalIndent(record, "", "  ")
	if err != nil {
		return err
	}
	_, err = m.store.WriteFileIfMatch(ctx, hubaccess.NameRecordKey(name), data, etag)
	return err
}

func (m *RegistryManager) refreshNameRecord(ctx context.Context, name string, project *Project) error {
	value, err := m.store.ReadValue(ctx, hubaccess.NameRecordKey(name))
	if err != nil {
		return err
	}
	var current nameRecord
	if json.Unmarshal(value.Data, &current) != nil || current.ProjectID != project.ID || current.Status != "active" {
		return fmt.Errorf("name record %q is not owned by project %s", name, project.ID)
	}
	record := nameRecord{Version: hubManifestVersion, Name: name, ProjectID: project.ID, ProjectRevision: project.Revision, Status: "active"}
	data, err := json.MarshalIndent(record, "", "  ")
	if err != nil {
		return err
	}
	_, err = m.store.WriteFileIfMatch(ctx, hubaccess.NameRecordKey(name), data, value.ETag)
	return err
}

func (m *RegistryManager) writeProjectCAS(ctx context.Context, key, etag string, project *Project) error {
	data, err := json.MarshalIndent(projectFile{Version: hubManifestVersion, Project: project}, "", "  ")
	if err != nil {
		return err
	}
	_, err = m.store.WriteFileIfMatch(ctx, key, data, etag)
	return err
}

func (m *RegistryManager) removeOwnedName(ctx context.Context, name, projectID string) error {
	value, err := m.store.ReadValue(ctx, hubaccess.NameRecordKey(name))
	if errors.Is(err, s3store.ErrNotFound) {
		return nil
	}
	if err != nil {
		return err
	}
	var record nameRecord
	if json.Unmarshal(value.Data, &record) != nil || record.ProjectID != projectID {
		return fmt.Errorf("name record %q is not owned by project %s", name, projectID)
	}
	return m.store.RemoveFileIfMatch(ctx, hubaccess.NameRecordKey(name), value.ETag)
}

func (m *RegistryManager) EnsureArtifactClone(ctx context.Context, artType ArtifactType, entryID, version, projectID string) (string, error) {
	if m.store == nil {
		return "", fmt.Errorf("hub not configured")
	}
	if err := m.authorizeProject(ctx, projectID); err != nil {
		return "", err
	}

	cloneDir, err := m.store.DownloadArtifact(ctx, artType, entryID, version, projectID)
	if err != nil {
		return "", fmt.Errorf("ensuring artifact clone %s@%s (%s): %w", entryID, version, artType, err)
	}

	return cloneDir, nil
}

func prepareASTPublish(srcDir, storageURI string, projectCfg config.ConfigMap, logger *slog.Logger) (string, error) {
	return prepareASTPublishVersion(context.Background(), srcDir, storageURI, projectCfg, logger, false, config.ResolveHubS3(nil, projectCfg))
}

func prepareASTPublishVersion(ctx context.Context, srcDir, storageURI string, projectCfg config.ConfigMap, logger *slog.Logger, latestOnly bool, s3Cfg config.S3Config) (string, error) {
	tmpDir, err := os.MkdirTemp("", brand.TempDirPrefix("ast-pub"))
	if err != nil {
		return "", err
	}
	candidates := []string{
		filepath.Join(srcDir, "graph.icebug"),
		store.ASTProjectIcebugDir(srcDir),
		srcDir,
	}
	var srcBundle string
	for _, c := range candidates {
		if info, err := os.Stat(filepath.Join(c, "schema.cypher")); err == nil && !info.IsDir() {
			srcBundle = c
			break
		}
	}
	if srcBundle != "" {
		dstBundle := ast.IcebugBundlePath(tmpDir)
		if err := os.MkdirAll(dstBundle, 0o755); err != nil {
			_ = os.RemoveAll(tmpDir)
			return "", err
		}
		entries, err := os.ReadDir(srcBundle)
		if err != nil {
			_ = os.RemoveAll(tmpDir)
			return "", fmt.Errorf("preparing AST publish: read bundle: %w", err)
		}
		for _, e := range entries {
			srcPath := filepath.Join(srcBundle, e.Name())
			dstPath := filepath.Join(dstBundle, e.Name())
			if e.IsDir() {
				continue
			}
			if e.Name() == "schema.cypher" {
				raw, err := os.ReadFile(srcPath)
				if err != nil {
					_ = os.RemoveAll(tmpDir)
					return "", err
				}
				old := string(raw)
				rewritten := rewriteIcebugStorageURI(old, storageURI)
				if err := os.WriteFile(dstPath, []byte(rewritten), 0o644); err != nil {
					_ = os.RemoveAll(tmpDir)
					return "", err
				}
			} else {
				if err := copyFile(srcPath, dstPath); err != nil {
					_ = os.RemoveAll(tmpDir)
					return "", fmt.Errorf("copy bundle file %s: %w", e.Name(), err)
				}
			}
		}
		srcSearch := filepath.Join(filepath.Dir(srcBundle), ast.SearchBundleDir)
		if _, err := os.Stat(srcSearch); err == nil {
			dstSearch := filepath.Join(tmpDir, ast.SearchBundleDir)
			if err := copyDir(srcSearch, dstSearch); err != nil {
				_ = os.RemoveAll(tmpDir)
				return "", fmt.Errorf("copy search index: %w", err)
			}
		} else {
			altSearch := storeASTSearchDir(srcDir)
			if _, err := os.Stat(altSearch); err == nil {
				dstSearch := filepath.Join(tmpDir, ast.SearchBundleDir)
				_ = copyDir(altSearch, dstSearch)
			}
		}
		if latestOnly {
			searchDir := filepath.Join(tmpDir, ast.SearchBundleDir)
			if info, err := os.Stat(searchDir); err == nil && info.IsDir() {
				if _, err := lancestore.CompactLatestSnapshot(ctx, lancestore.Config{URI: searchDir, S3: s3Cfg}); err != nil {
					_ = os.RemoveAll(tmpDir)
					return "", fmt.Errorf("compact AST search snapshot: %w", err)
				}
			}
		}
		return tmpDir, nil
	}

	_ = os.RemoveAll(tmpDir)
	return "", fmt.Errorf("preparing AST publish: no graph at %s (expected graph.icebug/schema.cypher)", srcDir)
}

func rewriteIcebugStorageURI(schema, newURI string) string {
	prefix := "storage = '"
	var out string
	rest := schema
	for {
		idx := strings.Index(rest, prefix)
		if idx < 0 {
			out += rest
			break
		}
		out += rest[:idx+len(prefix)]
		rest = rest[idx+len(prefix):]
		end := strings.Index(rest, "'")
		if end < 0 {
			out += rest
			break
		}
		out += newURI
		rest = rest[end:]
	}
	return out
}

func storeASTSearchDir(projectOrStore string) string {
	if info, err := os.Stat(filepath.Join(projectOrStore, "graph.icebug")); err == nil && info.IsDir() {
		return filepath.Join(projectOrStore, ast.SearchBundleDir)
	}
	return filepath.Join(store.ASTProjectDir(projectOrStore), ast.SearchBundleDir)
}

func (m *RegistryManager) IsReady() bool {
	return m.store != nil
}

func (m *RegistryManager) Store() *S3Store {
	return m.store
}

// MountsKnowledge reports whether a knowledge artifact can be read where it lives.
//
// It is a capability question, not a preference: it needs a configured bucket AND a binary with
// the search engine linked in. Without the engine there is nothing that can open an `s3://` index,
// so a build without the `lancedb` tag refuses the install instead of inventing a second store.
func (m *RegistryManager) MountsKnowledge() bool { return m.MountsArtifact(TypeKnowledge) }

// MountsArtifact reports whether an artifact of this type can be read where it lives.
//
// Both mountable families need the same two things — a configured bucket AND a binary with the
// search engine linked in — because both carry a search index, and without the engine there is
// nothing that can open an `s3://` one. Answering yes without it would install a context with no
// bytes and no way to read them.
func (m *RegistryManager) MountsArtifact(artType ArtifactType) bool {
	if !IsMountable(artType) {
		return false
	}
	return m.store != nil && m.store.Configured() && lancestore.Available()
}

func (m *RegistryManager) persistEntryFile(entry *Entry) error {
	if m.store == nil {
		return fmt.Errorf("hub not configured")
	}
	if err := hubaccess.ValidateProjectID(entry.ProjectID); err != nil {
		return err
	}

	ef := entryFile{
		Version: hubManifestVersion,
		Entry:   *entry,
	}

	data, err := json.MarshalIndent(ef, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing entry file: %w", err)
	}

	relPath := hubaccess.ProjectRegistryKey(entry.ProjectID, string(entry.Type), entry.ID)
	if err := m.store.WriteFile(m.baseCtx, relPath, data); err != nil {
		return fmt.Errorf("writing entry file %s: %w", relPath, err)
	}

	return nil
}

func copyDir(src, dst string) error {
	return filepath.Walk(src, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		rel, err := filepath.Rel(src, path)
		if err != nil {
			return err
		}
		destPath := filepath.Join(dst, rel)

		if info.IsDir() {
			return os.MkdirAll(destPath, info.Mode())
		}

		if err := os.MkdirAll(filepath.Dir(destPath), 0o755); err != nil {
			return err
		}
		return copyFile(path, destPath)
	})
}

func copyFile(src, dst string) error {
	ext := filepath.Ext(src)
	if ext == ".md" || ext == ".yaml" || ext == ".yml" || ext == ".txt" {
		return copyFileWithBrand(src, dst)
	}

	in, err := os.Open(src)
	if err != nil {
		return err
	}
	defer func() { _ = in.Close() }()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer func() { _ = out.Close() }()

	_, err = io.Copy(out, in)
	return err
}

func copyFileWithBrand(src, dst string) error {
	data, err := os.ReadFile(src)
	if err != nil {
		return err
	}

	if brand.Brand != "graphit" {
		content := string(data)
		r := strings.NewReplacer(
			"graphit", brand.Brand,
			"Graphit Code", brand.Capitalize(brand.Brand),
		)
		content = r.Replace(content)
		data = []byte(content)
	}

	return os.WriteFile(dst, data, 0o644)
}

// prepareKnowledgePublish stages what a knowledge artifact carries.
//
// It stages the wiki's Lance index directory as-is: the artifact is written by one project and
// never compiled by a consumer, so having every consumer re-derive it would repeat work.
//
// THE TABLES ARE ALL OF IT. A loop here also copied every `.md` beside them, on the reasoning that
// the pages were what a reader opens and nothing could reconstruct them without the sources. Both
// halves of that stopped being true: the pages are not written at all, and a page IS reconstructed
// from the tables — `ReadPageFrom` reads `chunks.body`, which is how a mounted artifact has been
// answering `wiki_source` since before this change. Leaving the loop in would have staged nothing
// and reported success.
//
// A memory wiki never reaches here, and must not: it is read-and-write and multi-writer, so it
// carries its source and a consumer extends it.
func prepareKnowledgePublishVersion(ctx context.Context, srcDir string, latestOnly bool, s3Cfg config.S3Config) (string, error) {
	tmpDir, err := os.MkdirTemp("", brand.TempDirPrefix("kn-pub"))
	if err != nil {
		return "", err
	}
	wikiDir := srcDir
	if info, err := os.Stat(wiki.WikiIndexPath(wikiDir)); err != nil || !info.IsDir() {
		wikiDir = store.KnowledgeProjectDir(srcDir)
	}
	if _, err := wiki.StagePublishedIndex(ctx, wikiDir, tmpDir); err != nil {
		_ = os.RemoveAll(tmpDir)
		return "", err
	}
	if latestOnly {
		if _, err := lancestore.CompactLatestSnapshot(ctx, lancestore.Config{URI: wiki.WikiIndexPath(tmpDir), S3: s3Cfg}); err != nil {
			_ = os.RemoveAll(tmpDir)
			return "", fmt.Errorf("compact knowledge snapshot: %w", err)
		}
	}
	return tmpDir, nil
}
