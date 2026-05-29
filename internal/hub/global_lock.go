package hub

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

const GlobalLockVersion = 2

const GlobalHubLockFile = "global.lock.json"

type ClusterMap map[string][]string

func (c *ClusterMap) UnmarshalJSON(data []byte) error {
	var m map[string][]string
	if err := json.Unmarshal(data, &m); err == nil {
		*c = m
		return nil
	}

	var raw map[string]any
	if err := json.Unmarshal(data, &raw); err != nil {
		return err
	}

	m = make(map[string][]string)
	for k, v := range raw {
		switch val := v.(type) {
		case string:
			m[k] = []string{val}
		case []any:
			var strs []string
			for _, item := range val {
				if s, ok := item.(string); ok {
					strs = append(strs, s)
				}
			}
			m[k] = strs
		}
	}
	*c = m
	return nil
}

type ProjectEntry struct {
	Instances []InstanceEntry `json:"instances"`
}

type InstanceEntry struct {
	Dir          string     `json:"dir"`
	Name         string     `json:"name,omitempty"`
	Description  string     `json:"description,omitempty"`
	Cluster      ClusterMap `json:"cluster,omitempty"`
	RegisteredAt string     `json:"registeredAt"`
}

type GlobalHubLock struct {
	Version   int                        `json:"version"`
	Projects  map[string]*ProjectEntry   `json:"projects"`
	Artifacts map[string]*GlobalArtifact `json:"artifacts"`
}

type GlobalArtifact struct {
	ID          string                     `json:"id"`
	Name        string                     `json:"name"`
	Description string                     `json:"description,omitempty"`
	Version     string                     `json:"version"`
	Hash        string                     `json:"hash,omitempty"`
	Type        ArtifactType               `json:"type"`
	CachePath   string                     `json:"cachePath"`
	Projects    map[string]*ProjectInstall `json:"projects"`
}

type ProjectInstall struct {
	ProjectDir  string `json:"projectDir"`
	LocalPath   string `json:"localPath"`
	InstalledAt string `json:"installedAt"`
}

type GlobalLockManager struct {
	Logger   *slog.Logger
	mu       sync.Mutex
	lockPath string
}

func (m *GlobalLockManager) log() *slog.Logger { return slogutil.Resolve(m.Logger) }

func NewGlobalLockManager() (*GlobalLockManager, error) {
	globalDir := brand.GlobalDir()
	if globalDir == "" {
		return nil, fmt.Errorf("cannot resolve global dir (~/.%s)", brand.Brand)
	}
	lockPath := filepath.Join(globalDir, GlobalHubLockFile)
	return &GlobalLockManager{lockPath: lockPath}, nil
}

func (m *GlobalLockManager) LockPath() string { return m.lockPath }

func (m *GlobalLockManager) Load() (*GlobalHubLock, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	return m.load()
}

func (m *GlobalLockManager) load() (*GlobalHubLock, error) {
	data, err := os.ReadFile(m.lockPath)
	if os.IsNotExist(err) {
		return &GlobalHubLock{
			Version:   GlobalLockVersion,
			Projects:  make(map[string]*ProjectEntry),
			Artifacts: make(map[string]*GlobalArtifact),
		}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("reading global hub lock: %w", err)
	}
	var lock GlobalHubLock
	if err := json.Unmarshal(data, &lock); err != nil {
		return nil, fmt.Errorf("parsing global hub lock: %w", err)
	}
	if lock.Projects == nil {
		lock.Projects = make(map[string]*ProjectEntry)
	}
	if lock.Artifacts == nil {
		lock.Artifacts = make(map[string]*GlobalArtifact)
	}
	return &lock, nil
}

func (m *GlobalLockManager) save(lock *GlobalHubLock) error {
	if err := os.MkdirAll(filepath.Dir(m.lockPath), 0o755); err != nil {
		return err
	}
	data, err := json.MarshalIndent(lock, "", "  ")
	if err != nil {
		return fmt.Errorf("marshalling global hub lock: %w", err)
	}
	return os.WriteFile(m.lockPath, data, 0o644)
}

func (m *GlobalLockManager) RegisterInstall(
	id, version string,
	artType ArtifactType,
	name, description, hash string,
	cachePath, projectID, projectDir, localPath string,
) (*GlobalArtifact, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return nil, err
	}

	key := artifactKey(id, version, artType)
	art := lock.Artifacts[key]
	if art == nil {
		art = &GlobalArtifact{
			ID:          id,
			Name:        name,
			Description: description,
			Version:     version,
			Hash:        hash,
			Type:        artType,
			CachePath:   cachePath,
			Projects:    make(map[string]*ProjectInstall),
		}
		lock.Artifacts[key] = art
	} else {

		if name != "" {
			art.Name = name
		}
		if description != "" {
			art.Description = description
		}
		if hash != "" {
			art.Hash = hash
		}
		if cachePath != "" {
			art.CachePath = cachePath
		}
	}

	art.Projects[projectID] = &ProjectInstall{
		ProjectDir:  projectDir,
		LocalPath:   localPath,
		InstalledAt: time.Now().UTC().Format(time.RFC3339),
	}

	return art, m.save(lock)
}

func (m *GlobalLockManager) RegisterUninstall(id, version string, artType ArtifactType, projectID string) (orphaned bool, err error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return false, err
	}

	key := artifactKey(id, version, artType)
	art := lock.Artifacts[key]
	if art == nil {
		return false, nil
	}

	delete(art.Projects, projectID)

	if len(art.Projects) == 0 {
		delete(lock.Artifacts, key)
		orphaned = true
	}

	return orphaned, m.save(lock)
}

func (m *GlobalLockManager) GCOrphans() ([]string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return nil, err
	}

	var removed []string
	for key, art := range lock.Artifacts {
		if len(art.Projects) == 0 {
			if art.CachePath != "" {
				if err := os.RemoveAll(art.CachePath); err == nil {
					removed = append(removed, art.CachePath)
				}
			}
			delete(lock.Artifacts, key)
		}
	}

	return removed, m.save(lock)
}

func (m *GlobalLockManager) ListInstalledInProject(projectID string) ([]*GlobalArtifact, error) {
	lock, err := m.Load()
	if err != nil {
		return nil, err
	}
	var result []*GlobalArtifact
	for _, art := range lock.Artifacts {
		if _, ok := art.Projects[projectID]; ok {
			result = append(result, art)
		}
	}
	return result, nil
}

func (m *GlobalLockManager) ValidateProjectDirs() (int, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return 0, err
	}

	cleaned := 0

	for id, entry := range lock.Projects {
		valid := entry.Instances[:0]
		for _, inst := range entry.Instances {
			lockFilePath := filepath.Join(inst.Dir, brand.LockFileName())
			if _, err := os.Stat(lockFilePath); os.IsNotExist(err) {
				cleaned++
				continue
			}
			valid = append(valid, inst)
		}
		entry.Instances = valid
		if len(entry.Instances) == 0 {
			delete(lock.Projects, id)
		}
	}

	for key, art := range lock.Artifacts {
		for projID, proj := range art.Projects {
			lockFilePath := filepath.Join(proj.ProjectDir, brand.LockFileName())
			if _, err := os.Stat(lockFilePath); os.IsNotExist(err) {
				delete(art.Projects, projID)
				cleaned++
			}
		}
		if len(art.Projects) == 0 {
			delete(lock.Artifacts, key)
		}
	}

	if cleaned > 0 {
		return cleaned, m.save(lock)
	}
	return 0, nil
}

func (m *GlobalLockManager) RegisterProject(projectID, projectDir string, opts ...func(*InstanceEntry)) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return err
	}

	absDir, _ := filepath.Abs(projectDir)

	entry := lock.Projects[projectID]
	if entry == nil {
		entry = &ProjectEntry{}
		lock.Projects[projectID] = entry
	}

	idx := -1
	for i := range entry.Instances {
		instAbs, _ := filepath.Abs(entry.Instances[i].Dir)
		if instAbs == absDir {
			idx = i
			break
		}
	}

	if idx >= 0 {
		inst := &entry.Instances[idx]
		for _, opt := range opts {
			opt(inst)
		}
	} else {
		inst := InstanceEntry{
			Dir:          projectDir,
			RegisteredAt: time.Now().UTC().Format(time.RFC3339),
		}
		for _, opt := range opts {
			opt(&inst)
		}
		entry.Instances = append(entry.Instances, inst)
	}
	return m.save(lock)
}

func WithProjectName(name string) func(*InstanceEntry) {
	return func(e *InstanceEntry) { e.Name = name }
}

func WithProjectDescription(desc string) func(*InstanceEntry) {
	return func(e *InstanceEntry) { e.Description = desc }
}

func (m *GlobalLockManager) SetCluster(projectID, projectDir, key, value string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return err
	}

	inst := m.findOrCreateInstance(lock, projectID, projectDir)
	if inst.Cluster == nil {
		inst.Cluster = make(map[string][]string)
	}
	for _, existing := range inst.Cluster[key] {
		if existing == value {
			return nil
		}
	}
	inst.Cluster[key] = append(inst.Cluster[key], value)
	return m.save(lock)
}

func (m *GlobalLockManager) UnsetCluster(projectID, projectDir, key string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return err
	}

	inst := m.findInstance(lock, projectID, projectDir)
	if inst == nil {
		return nil
	}
	delete(inst.Cluster, key)
	if len(inst.Cluster) == 0 {
		inst.Cluster = nil
	}
	return m.save(lock)
}

func (m *GlobalLockManager) GetCluster(projectID, projectDir, key string) ([]string, error) {
	lock, err := m.Load()
	if err != nil {
		return nil, err
	}
	inst := m.findInstance(lock, projectID, projectDir)
	if inst == nil || inst.Cluster == nil {
		return nil, nil
	}
	return inst.Cluster[key], nil
}

func (m *GlobalLockManager) GetAllClusterLabels(projectID, projectDir string) (map[string][]string, error) {
	lock, err := m.Load()
	if err != nil {
		return nil, err
	}
	inst := m.findInstance(lock, projectID, projectDir)
	if inst == nil {
		return nil, nil
	}
	return inst.Cluster, nil
}

func (m *GlobalLockManager) UnregisterProject(projectID, projectDir string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return err
	}

	entry := lock.Projects[projectID]
	if entry == nil {
		return nil
	}

	absDir, _ := filepath.Abs(projectDir)
	filtered := entry.Instances[:0]
	for _, inst := range entry.Instances {
		instAbs, _ := filepath.Abs(inst.Dir)
		if instAbs != absDir {
			filtered = append(filtered, inst)
		}
	}
	entry.Instances = filtered

	if len(entry.Instances) == 0 {
		delete(lock.Projects, projectID)
	}
	return m.save(lock)
}

type ActiveProject struct {
	ID  string
	Dir string
}

func (m *GlobalLockManager) ListActiveProjects() ([]ActiveProject, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return nil, err
	}

	var active []ActiveProject
	dirty := false

	for id, entry := range lock.Projects {
		valid := entry.Instances[:0]
		for _, inst := range entry.Instances {
			lockFilePath := filepath.Join(inst.Dir, brand.LockFileName())
			if _, err := os.Stat(lockFilePath); os.IsNotExist(err) {
				dirty = true
				continue
			}
			valid = append(valid, inst)
			active = append(active, ActiveProject{ID: id, Dir: inst.Dir})
		}
		entry.Instances = valid
		if len(entry.Instances) == 0 {
			delete(lock.Projects, id)
		}
	}

	if dirty {
		if err := m.save(lock); err != nil {
			m.log().Warn("save global lock (stale cleanup)", "error", err)
		}
	}

	return active, nil
}

func (m *GlobalLockManager) findInstance(lock *GlobalHubLock, projectID, projectDir string) *InstanceEntry {
	entry := lock.Projects[projectID]
	if entry == nil {
		return nil
	}
	absDir, _ := filepath.Abs(projectDir)
	for i := range entry.Instances {
		instAbs, _ := filepath.Abs(entry.Instances[i].Dir)
		if instAbs == absDir {
			return &entry.Instances[i]
		}
	}
	return nil
}

func (m *GlobalLockManager) findOrCreateInstance(lock *GlobalHubLock, projectID, projectDir string) *InstanceEntry {
	entry := lock.Projects[projectID]
	if entry == nil {
		entry = &ProjectEntry{}
		lock.Projects[projectID] = entry
	}
	absDir, _ := filepath.Abs(projectDir)
	for i := range entry.Instances {
		instAbs, _ := filepath.Abs(entry.Instances[i].Dir)
		if instAbs == absDir {
			return &entry.Instances[i]
		}
	}
	entry.Instances = append(entry.Instances, InstanceEntry{
		Dir:          projectDir,
		RegisteredAt: time.Now().UTC().Format(time.RFC3339),
	})
	return &entry.Instances[len(entry.Instances)-1]
}

func artifactKey(id, version string, artType ArtifactType) string {
	return string(artType) + "/" + id + "@" + version
}

type InstalledArtifactInfo struct {
	ID          string
	Name        string
	Type        ArtifactType
	Description string
	Version     string
	ProjectID   string
}

func LoadInstalledArtifacts() []InstalledArtifactInfo {
	pp := paths.GetPaths("", false)
	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		return nil
	}

	var globalArts map[string]*GlobalArtifact
	if mgr, err := NewGlobalLockManager(); err == nil {
		if glock, err := mgr.Load(); err == nil {
			globalArts = glock.Artifacts
		}
	}

	var registry *RegistryManager
	registry, _ = NewRegistryManager(context.Background())

	var result []InstalledArtifactInfo
	for artType, typeMap := range lf.Artifacts {
		for id, meta := range typeMap {
			if !meta.IsHubInstalled() {
				continue
			}

			name := meta.Alias
			if name == "" {
				name = id
			}
			var description string

			if globalArts != nil {
				key := artifactKey(id, meta.Version, artType)
				if ga, ok := globalArts[key]; ok {
					if ga.Name != "" {
						name = ga.Name
					}
					description = ga.Description
				}
			}

			if description == "" && registry != nil && registry.IsReady() {
				if entry := registry.GetEntry(id, artType); entry != nil {
					if entry.Description != "" {
						description = entry.Description
					}
					if name == id && entry.Name != "" {
						name = entry.Name
					}
				}
			}

			result = append(result, InstalledArtifactInfo{
				ID:          id,
				Name:        name,
				Type:        artType,
				Description: description,
				Version:     meta.Version,
				ProjectID:   meta.ProjectID,
			})
		}
	}

	sort.Slice(result, func(i, j int) bool {
		if result[i].Type != result[j].Type {
			return result[i].Type < result[j].Type
		}
		return result[i].ID < result[j].ID
	})
	return result
}
