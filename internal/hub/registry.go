package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	paths_pkg "github.com/graphit-labs/graphit-code/internal/paths"
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
	RemoteID    string `json:"remote_id"`
	Name        string `json:"name"`
	Description string `json:"description,omitempty"`
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

type RegistryCache struct {
	V        int                `json:"v"`
	Commit   string             `json:"commit"`
	Projects map[string]Project `json:"projects"`
	Entries  []Entry            `json:"entries"`
}

type baselinesData struct {
	Baselines []Baseline `json:"baselines"`
}

const (
	hubManifestVersion = 1
	globalProjectKey   = "_global"
	registryCacheFile  = "hub.registry.json"
)

type RegistryManager struct {
	Logger        *slog.Logger
	store         *S3Store
	projectConfig config.ConfigMap

	baseCtx context.Context

	entries  map[ArtifactType]map[string]*Entry
	projects map[string]*Project

	registryPaths []string
}

func (r *RegistryManager) log() *slog.Logger { return slogutil.Resolve(r.Logger) }

func NewRegistryManager(ctx context.Context, paths ...string) (*RegistryManager, error) {
	m := &RegistryManager{
		entries:       make(map[ArtifactType]map[string]*Entry),
		projects:      make(map[string]*Project),
		registryPaths: paths,
		baseCtx:       ctx,
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
		if err := st.SyncRegistry(ctx); err != nil {
			m.log().Warn("sync registry", "error", err)
			m.store = nil
		} else if err := m.loadRegistry(); err != nil {
			m.log().Warn("load registry", "error", err)
		}
	}

	m.loadLocalRegistries()
	return m, nil
}

func (m *RegistryManager) loadRegistry() error {
	if m.store == nil {
		return fmt.Errorf("git store not initialized")
	}

	cache, err := m.LoadRegistryCache()
	if err == nil && cache != nil {
		headCommit := m.store.RegistryRevision(m.baseCtx)
		if cache.Commit != "" && cache.Commit == headCommit {
			m.loadFromCacheData(cache)
			return nil
		}
	}

	newCache, err := m.BuildRegistryCache()
	if err != nil {
		return err
	}
	m.loadFromCacheData(newCache)
	return nil
}

func (m *RegistryManager) loadFromCacheData(cache *RegistryCache) {
	for rid, p := range cache.Projects {
		p := p
		if rid != globalProjectKey {
			m.projects[rid] = &p
		}
	}
	for _, entry := range cache.Entries {
		entry := entry
		t := entry.Type
		if m.entries[t] == nil {
			m.entries[t] = make(map[string]*Entry)
		}
		m.entries[t][entry.ID] = &entry
	}
}

func (m *RegistryManager) BuildRegistryCache() (*RegistryCache, error) {
	if m.store == nil {
		return nil, fmt.Errorf("git store not initialized")
	}

	cache := &RegistryCache{
		V:        hubManifestVersion,
		Commit:   m.store.RegistryRevision(m.baseCtx),
		Projects: make(map[string]Project),
		Entries:  []Entry{},
	}

	projectsDir := m.store.AbsPath("projects")

	topEntries, err := os.ReadDir(projectsDir)
	if err != nil {

		if err := m.SaveRegistryCache(cache); err != nil {
			return nil, err
		}
		return cache, nil
	}

	for _, topEntry := range topEntries {
		topPath := filepath.Join(projectsDir, topEntry.Name())

		if topEntry.IsDir() {

			children, err := os.ReadDir(topPath)
			if err != nil {
				continue
			}
			for _, child := range children {
				if child.IsDir() {
					childPath := filepath.Join(topPath, child.Name())
					m.loadProjectDir(childPath, "", cache)
				}
			}
		}
	}

	sort.Slice(cache.Entries, func(i, j int) bool {
		if cache.Entries[i].Type != cache.Entries[j].Type {
			return cache.Entries[i].Type < cache.Entries[j].Type
		}
		return cache.Entries[i].ID < cache.Entries[j].ID
	})

	if err := m.SaveRegistryCache(cache); err != nil {
		return nil, err
	}

	return cache, nil
}

func (m *RegistryManager) loadProjectDir(dir string, knownID string, cache *RegistryCache) {

	projectID := knownID
	projData, err := os.ReadFile(filepath.Join(dir, "project.json"))
	if err == nil {
		var pf projectFile
		if json.Unmarshal(projData, &pf) == nil && pf.Project != nil {
			projectID = pf.Project.RemoteID
			cache.Projects[projectID] = *pf.Project
		}
	}

	dirEntries, err := os.ReadDir(dir)
	if err != nil {
		return
	}
	for _, de := range dirEntries {
		if de.IsDir() || de.Name() == "project.json" || !strings.HasSuffix(de.Name(), ".json") {
			continue
		}

		data, err := os.ReadFile(filepath.Join(dir, de.Name()))
		if err != nil {
			continue
		}

		var ef entryFile
		if err := json.Unmarshal(data, &ef); err != nil {
			continue
		}

		if ef.Entry.ProjectID == "" && projectID != "" {
			ef.Entry.ProjectID = projectID
		}

		if projectID == "" && ef.Entry.ProjectID != "" {
			projectID = ef.Entry.ProjectID
		}

		cache.Entries = append(cache.Entries, ef.Entry)
	}
}

func RegistryCachePath() string {
	return filepath.Join(brand.GlobalDir(), registryCacheFile)
}

func (m *RegistryManager) LoadRegistryCache() (*RegistryCache, error) {
	data, err := os.ReadFile(RegistryCachePath())
	if err != nil {
		return nil, err
	}
	var cache RegistryCache
	if err := json.Unmarshal(data, &cache); err != nil {
		return nil, err
	}
	return &cache, nil
}

func (m *RegistryManager) SaveRegistryCache(cache *RegistryCache) error {
	data, err := json.MarshalIndent(cache, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing registry cache: %w", err)
	}
	cachePath := RegistryCachePath()
	if err := os.MkdirAll(filepath.Dir(cachePath), 0o755); err != nil {
		return err
	}
	return os.WriteFile(cachePath, data, 0o644)
}

func (m *RegistryManager) loadLocalRegistries() {
	for _, path := range m.registryPaths {
		data, err := os.ReadFile(path)
		if err != nil {
			continue
		}
		type flatRegistry struct {
			Entries []Entry `json:"entries"`
		}
		var rd flatRegistry
		if json.Unmarshal(data, &rd) == nil {
			for _, entry := range rd.Entries {
				entry := entry
				t := entry.Type
				if m.entries[t] == nil {
					m.entries[t] = make(map[string]*Entry)
				}
				m.entries[t][entry.ID] = &entry
			}
		}
	}
}

func (m *RegistryManager) GetEntry(id string, entryType ArtifactType) *Entry {
	if entryType != "" {
		e := m.entries[entryType][id]
		return e
	}
	for _, typeMap := range m.entries {
		if e, ok := typeMap[id]; ok {
			return e
		}
	}
	return nil
}

func (m *RegistryManager) ListEntries(typeFilter ArtifactType) []*Entry {
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

func (m *RegistryManager) SearchEntries(term string, typeFilter ArtifactType) []*Entry {
	lower := strings.ToLower(term)

	var nameMatches, descMatches []*Entry
	for t, typeMap := range m.entries {
		if typeFilter != "" && t != typeFilter {
			continue
		}
		for _, e := range typeMap {
			nameLower := strings.ToLower(e.Name)
			descLower := strings.ToLower(e.Description)
			idLower := strings.ToLower(e.ID)

			if strings.Contains(nameLower, lower) || strings.Contains(idLower, lower) {
				nameMatches = append(nameMatches, e)
			} else if strings.Contains(descLower, lower) {
				descMatches = append(descMatches, e)
			}
		}
	}

	sortEntries := func(entries []*Entry) {
		sort.Slice(entries, func(i, j int) bool {
			if entries[i].Type != entries[j].Type {
				return entries[i].Type < entries[j].Type
			}
			return entries[i].ID < entries[j].ID
		})
	}
	sortEntries(nameMatches)
	sortEntries(descMatches)

	return append(nameMatches, descMatches...)
}

func (m *RegistryManager) ListProjects() []*Project {
	result := make([]*Project, 0, len(m.projects))
	for _, p := range m.projects {
		result = append(result, p)
	}
	return result
}

func (m *RegistryManager) GetProject(remoteID string) *Project {
	return m.projects[remoteID]
}

func (m *RegistryManager) GetProjectByName(name string) *Project {
	for _, p := range m.projects {
		if p.Name == name {
			return p
		}
	}
	return nil
}

func (m *RegistryManager) GetDefaultBaselines(_ context.Context) ([]Baseline, error) {
	if m.store == nil {
		return nil, fmt.Errorf("hub not configured")
	}
	data, err := m.store.ReadFile(m.baseCtx, "baselines.json")
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

	if err := m.store.SyncRegistry(ctx); err != nil {
		return err
	}

	versionHash, err := HashDirectory(localPath)
	if err != nil {
		return fmt.Errorf("computing artifact hash: %w", err)
	}

	publishPath := localPath

	switch meta.Type {
	case TypeAST:
		storageURI := m.store.ArtifactURI(TypeAST, entryID, version, meta.ProjectID, ast.IcebugBundleDir)
		if storageURI == "" {
			return fmt.Errorf("preparing AST publish: the hub is not configured, so there is no " +
				"location to point the published graph at")
		}
		prepared, err := prepareASTPublish(localPath, storageURI, m.projectConfig, m.Logger)
		if err != nil {
			return fmt.Errorf("preparing AST publish: %w", err)
		}
		defer func() { _ = os.RemoveAll(prepared) }()
		publishPath = prepared
	case TypeKnowledge:
		prepared, err := prepareKnowledgePublish(ctx, localPath)
		if err != nil {
			return fmt.Errorf("preparing knowledge publish: %w", err)
		}
		defer func() { _ = os.RemoveAll(prepared) }()
		publishPath = prepared
	}

	if err := m.store.PublishArtifact(ctx, meta.Type, entryID, version, meta.ProjectID, publishPath); err != nil {
		return fmt.Errorf("publishing artifact to branch: %w", err)
	}

	existing := m.entries[meta.Type][entryID]
	if existing == nil {
		existing = &Entry{}
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

	if m.entries[meta.Type] == nil {
		m.entries[meta.Type] = make(map[string]*Entry)
	}
	m.entries[meta.Type][entryID] = meta

	if err := m.persistEntryFile(meta); err != nil {
		return err
	}
	if err := m.persistProjectFile(meta.ProjectID); err != nil {
		return err
	}

	if _, err := m.BuildRegistryCache(); err != nil {
		m.log().Warn("rebuild cache after publish", "error", err)
	}

	return nil
}

func (m *RegistryManager) DeleteEntry(ctx context.Context, entryID string, entryType ArtifactType) error {
	if m.store == nil {
		return fmt.Errorf("hub not configured")
	}

	if err := m.store.SyncRegistry(ctx); err != nil {
		return err
	}

	entry := m.GetEntry(entryID, entryType)
	if entry == nil {
		return fmt.Errorf("entry %q not found in registry", entryID)
	}

	projKey := entry.ProjectID
	if projKey == "" {
		projKey = globalProjectKey
	}

	for _, ver := range entry.Versions {
		if err := m.store.DeleteArtifact(ctx, entry.Type, entryID, ver, entry.ProjectID); err != nil {
			m.log().Warn("delete branch", "id", entryID, "version", ver, "error", err)
		}
	}

	entryRelPath := projectDir(projKey) + "/" + sanitizeEntryFileName(entry)
	if err := m.store.RemoveFile(ctx, entryRelPath); err != nil {
		m.log().Warn("remove entry file", "path", entryRelPath, "error", err)
	}

	delete(m.entries[entry.Type], entryID)
	if len(m.entries[entry.Type]) == 0 {
		delete(m.entries, entry.Type)
	}

	if _, err := m.BuildRegistryCache(); err != nil {
		m.log().Warn("rebuild cache after delete", "error", err)
	}

	return nil
}

func (m *RegistryManager) UpsertProject(ctx context.Context, remoteID, name, description string) (*Project, error) {
	if remoteID == "" {
		return nil, fmt.Errorf("remoteID must not be empty")
	}
	if name == "" {
		return nil, fmt.Errorf("project name must not be empty")
	}

	for rid, p := range m.projects {
		if rid != remoteID && p.Name == name {
			return nil, fmt.Errorf("project name %q is already used by project %q", name, rid)
		}
	}

	proj := &Project{RemoteID: remoteID, Name: name, Description: description}
	if existing, ok := m.projects[remoteID]; ok {
		proj = existing
		proj.Name = name
		proj.Description = description
	}
	m.projects[remoteID] = proj

	if m.store == nil {
		return proj, nil
	}

	if err := m.persistProjectFile(remoteID); err != nil {
		return nil, err
	}

	if _, err := m.BuildRegistryCache(); err != nil {
		m.log().Warn("rebuild cache after upsert", "error", err)
	}

	return proj, nil
}

func (m *RegistryManager) EnsureArtifactClone(ctx context.Context, artType ArtifactType, entryID, version, projectID string) (string, error) {
	if m.store == nil {
		return "", fmt.Errorf("hub not configured")
	}

	cloneDir, err := m.store.DownloadArtifact(ctx, artType, entryID, version, projectID)
	if err != nil {
		return "", fmt.Errorf("ensuring artifact clone %s@%s (%s): %w", entryID, version, artType, err)
	}

	return cloneDir, nil
}

func prepareASTPublish(srcDir, storageURI string, projectCfg config.ConfigMap, logger *slog.Logger) (string, error) {
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

func projectDir(remoteID string) string {
	if remoteID == "" {
		remoteID = globalProjectKey
	}
	hash := sha256.Sum256([]byte(remoteID))
	hexHash := hex.EncodeToString(hash[:])
	return "projects/" + hexHash[:2] + "/" + hexHash
}

var unsafeCharsRe = regexp.MustCompile(`[^a-z0-9._-]+`)

func sanitizeForFileName(s string) string {
	s = strings.ToLower(s)
	s = unsafeCharsRe.ReplaceAllString(s, "-")
	s = strings.Trim(s, "-")
	if s == "" {
		s = "_"
	}
	return s
}

func sanitizeEntryFileName(entry *Entry) string {
	typ := sanitizeForFileName(string(entry.Type))
	ver := sanitizeForFileName(entry.Latest)

	if entry.Type == TypeAST || entry.Type == TypeKnowledge {
		return typ + "_" + ver + ".json"
	}

	name := sanitizeForFileName(entry.Name)
	if name == "" || name == "_" {
		name = sanitizeForFileName(entry.ID)
	}
	return typ + "_" + name + "_" + ver + ".json"
}

func (m *RegistryManager) persistEntryFile(entry *Entry) error {
	if m.store == nil {
		return fmt.Errorf("hub not configured")
	}

	projKey := entry.ProjectID
	if projKey == "" {
		projKey = globalProjectKey
	}

	ef := entryFile{
		Version: hubManifestVersion,
		Entry:   *entry,
	}

	data, err := json.MarshalIndent(ef, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing entry file: %w", err)
	}

	relPath := projectDir(projKey) + "/" + sanitizeEntryFileName(entry)
	if err := m.store.WriteFile(m.baseCtx, relPath, data); err != nil {
		return fmt.Errorf("writing entry file %s: %w", relPath, err)
	}

	return nil
}

func (m *RegistryManager) persistProjectFile(remoteID string) error {
	if m.store == nil {
		return fmt.Errorf("hub not configured")
	}

	if remoteID == "" {
		remoteID = globalProjectKey
	}

	relPath := projectDir(remoteID) + "/project.json"

	pf := projectFile{Version: hubManifestVersion}
	if existing, err := m.store.ReadFile(m.baseCtx, relPath); err == nil {
		var existingPF projectFile
		if json.Unmarshal(existing, &existingPF) == nil {
			pf = existingPF
		}
	}

	if p := m.projects[remoteID]; p != nil && remoteID != globalProjectKey {
		pf.Project = p
	}
	pf.Version = hubManifestVersion

	data, err := json.MarshalIndent(pf, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing project file: %w", err)
	}

	if err := m.store.WriteFile(m.baseCtx, relPath, data); err != nil {
		return fmt.Errorf("writing project file %s: %w", relPath, err)
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
// It stages the wiki's Lance index directory as-is: the artifact is written by one project, pinned
// by its version and never compiled by a consumer, so having every consumer re-derive the same
// frozen index would repeat work for a value already computed.
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
func prepareKnowledgePublish(ctx context.Context, srcDir string) (string, error) {
	tmpDir, err := os.MkdirTemp("", brand.TempDirPrefix("kn-pub"))
	if err != nil {
		return "", err
	}
	if _, err := wiki.StagePublishedIndex(ctx, srcDir, tmpDir); err != nil {
		_ = os.RemoveAll(tmpDir)
		return "", err
	}
	return tmpDir, nil
}
