package hub

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	paths_pkg "github.com/graphit-labs/graphit-code/internal/paths"
)

type ArtifactType string

const (
	TypeAgent     ArtifactType = "agent"
	TypeRule      ArtifactType = "rule"
	TypeWorkflow  ArtifactType = "workflow"
	TypeSkill     ArtifactType = "skill"
	TypeKnowledge ArtifactType = "knowledge"
	TypeAST       ArtifactType = "ast"
	TypeMCP       ArtifactType = "mcp"
	TypeCommand   ArtifactType = "command"
	TypePower     ArtifactType = "power"
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
}

var ValidTypes = []ArtifactType{
	TypeAgent, TypeRule, TypeWorkflow, TypeSkill,
	TypeKnowledge, TypeAST, TypeMCP, TypeCommand, TypePower,
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
	gitStore *GitStore

	entries  map[ArtifactType]map[string]*Entry
	projects map[string]*Project

	registryPaths []string
}

func NewRegistryManager(ctx context.Context, paths ...string) (*RegistryManager, error) {
	m := &RegistryManager{
		entries:       make(map[ArtifactType]map[string]*Entry),
		projects:      make(map[string]*Project),
		registryPaths: paths,
	}

	var projectCfg config.ConfigMap
	pp := paths_pkg.GetPaths("", false)
	if lf, err := LoadLockfile(pp.LockFilePath); err == nil && lf != nil {
		projectCfg = lf.Config
	}

	gs, err := NewGitStore(nil, projectCfg)
	if err == nil {
		m.gitStore = gs
		if err := gs.EnsureCloned(); err != nil {

			m.gitStore = nil
		} else {
			if err := m.loadRegistry(); err != nil {
				fmt.Fprintf(os.Stderr, "[hub] load registry: %v\n", err)
			}
		}
	}

	m.loadLocalRegistries()
	return m, nil
}

func (m *RegistryManager) loadRegistry() error {
	if m.gitStore == nil {
		return fmt.Errorf("git store not initialized")
	}

	cache, err := m.LoadRegistryCache()
	if err == nil && cache != nil {
		headCommit := m.gitStore.HeadCommit()
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
	if m.gitStore == nil {
		return nil, fmt.Errorf("git store not initialized")
	}

	cache := &RegistryCache{
		V:        hubManifestVersion,
		Commit:   m.gitStore.HeadCommit(),
		Projects: make(map[string]Project),
		Entries:  []Entry{},
	}

	projectsDir := m.gitStore.AbsPath("projects")

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
	if m.gitStore == nil {
		return nil, fmt.Errorf("hub not configured")
	}
	data, err := m.gitStore.ReadFile("baselines.json")
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
	if m.gitStore == nil {
		return fmt.Errorf("hub not configured — run '%s setup' first", brand.BinName())
	}

	if err := m.gitStore.Sync(); err != nil {
		return err
	}

	versionHash, err := HashDirectory(localPath)
	if err != nil {
		return fmt.Errorf("computing artifact hash: %w", err)
	}

	publishPath := localPath

	if meta.Type == TypeAST {
		prepared, err := prepareASTPublish(localPath)
		if err != nil {
			return fmt.Errorf("preparing AST publish: %w", err)
		}
		defer os.RemoveAll(prepared)
		publishPath = prepared
	}

	if err := m.gitStore.WriteArtifactBranch(meta.Type, entryID, version, meta.ProjectID, publishPath); err != nil {
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

	if err := m.gitStore.CommitAndPush(fmt.Sprintf("publish %s@%s (%s)", entryID, version, meta.Type)); err != nil {
		return err
	}

	if _, err := m.BuildRegistryCache(); err != nil {
		fmt.Fprintf(os.Stderr, "[hub] rebuild cache after publish: %v\n", err)
	}

	return nil
}

func (m *RegistryManager) DeleteEntry(ctx context.Context, entryID string, entryType ArtifactType) error {
	if m.gitStore == nil {
		return fmt.Errorf("hub not configured")
	}

	if err := m.gitStore.Sync(); err != nil {
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
		if err := m.gitStore.DeleteArtifactBranch(entry.Type, entryID, ver, entry.ProjectID); err != nil {
			fmt.Fprintf(os.Stderr, "[hub] delete branch %s@%s: %v\n", entryID, ver, err)
		}
	}

	entryFileName := sanitizeEntryFileName(entry)
	projDir := m.gitStore.AbsPath(projectDir(projKey))
	entryFilePath := filepath.Join(projDir, entryFileName)
	if err := os.Remove(entryFilePath); err != nil && !os.IsNotExist(err) {
		fmt.Fprintf(os.Stderr, "[hub] remove entry file %s: %v\n", entryFilePath, err)
	}

	delete(m.entries[entry.Type], entryID)
	if len(m.entries[entry.Type]) == 0 {
		delete(m.entries, entry.Type)
	}

	if err := m.gitStore.CommitAndPush(fmt.Sprintf("delete %s (%s)", entryID, entry.Type)); err != nil {
		return err
	}

	if _, err := m.BuildRegistryCache(); err != nil {
		fmt.Fprintf(os.Stderr, "[hub] rebuild cache after delete: %v\n", err)
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

	if m.gitStore == nil {
		return proj, nil
	}

	if err := m.persistProjectFile(remoteID); err != nil {
		return nil, err
	}

	if err := m.gitStore.CommitAndPush(fmt.Sprintf("upsert project %s", name)); err != nil {
		return nil, fmt.Errorf("persisting project: %w", err)
	}

	if _, err := m.BuildRegistryCache(); err != nil {
		fmt.Fprintf(os.Stderr, "[hub] rebuild cache after upsert: %v\n", err)
	}

	return proj, nil
}

func (m *RegistryManager) EnsureArtifactClone(_ context.Context, artType ArtifactType, entryID, version, projectID string) (string, error) {
	if m.gitStore == nil {
		return "", fmt.Errorf("hub not configured")
	}

	cloneDir, err := m.gitStore.EnsureArtifactClone(artType, entryID, version, projectID)
	if err != nil {
		return "", fmt.Errorf("ensuring artifact clone %s@%s (%s): %w", entryID, version, artType, err)
	}

	return cloneDir, nil
}

func prepareASTPublish(srcDir string) (string, error) {
	tmpDir, err := os.MkdirTemp("", brand.TempDirPrefix("ast-pub"))
	if err != nil {
		return "", err
	}

	manifestSrc := filepath.Join(srcDir, "manifest.json")
	if data, err := os.ReadFile(manifestSrc); err == nil {
		if err := os.WriteFile(filepath.Join(tmpDir, "manifest.json"), data, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "[hub] ast-publish: write manifest: %v\n", err)
		}
	}

	shardsDir := filepath.Join(srcDir, "shards")
	if info, err := os.Stat(shardsDir); err == nil && info.IsDir() {
		_ = filepath.Walk(shardsDir, func(path string, fi os.FileInfo, walkErr error) error {
			if walkErr != nil || fi.IsDir() {
				rel, _ := filepath.Rel(srcDir, path)
				_ = os.MkdirAll(filepath.Join(tmpDir, rel), 0o755)
				return nil
			}
			if fi.Name() == ".git" {
				return filepath.SkipDir
			}
			rel, _ := filepath.Rel(srcDir, path)
			data, readErr := os.ReadFile(path)
			if readErr != nil {
				return nil
			}
			dest := filepath.Join(tmpDir, rel)
			_ = os.MkdirAll(filepath.Dir(dest), 0o755)
			_ = os.WriteFile(dest, data, 0o644)
			return nil
		})
	}

	return tmpDir, nil
}

func (m *RegistryManager) IsReady() bool {
	return m.gitStore != nil
}

func (m *RegistryManager) GitStore() *GitStore {
	return m.gitStore
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
	if m.gitStore == nil {
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
	if err := m.gitStore.WriteFile(relPath, data); err != nil {
		return fmt.Errorf("writing entry file %s: %w", relPath, err)
	}

	return nil
}

func (m *RegistryManager) persistProjectFile(remoteID string) error {
	if m.gitStore == nil {
		return fmt.Errorf("hub not configured")
	}

	if remoteID == "" {
		remoteID = globalProjectKey
	}

	relPath := projectDir(remoteID) + "/project.json"

	pf := projectFile{Version: hubManifestVersion}
	if existing, err := m.gitStore.ReadFile(relPath); err == nil {
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

	if err := m.gitStore.WriteFile(relPath, data); err != nil {
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
	defer in.Close()

	out, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer out.Close()

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
