package hub

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
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
	ID          string       `json:"id"`
	Name        string       `json:"name"`
	Description string       `json:"description,omitempty"`
	Version     string       `json:"version"`
	Hash        string       `json:"hash,omitempty"`
	Type        ArtifactType `json:"type"`
	CachePath   string       `json:"cachePath"`
	// ProjectID is the PUBLISHING project, the same meaning it has in
	// LockfileArtifactMeta — not one of the consumers in Projects below.
	//
	// It is recorded because it is half of a store's address: a Hub context is named
	// after the project that published it (store.ContextNameFor), so resolving
	// ASTHubDir or KnowledgeHubDir from this entry alone is impossible without it.
	// A project-scoped install never needed it here, having the same field in its own
	// lockfile; a project-less install has no lockfile, and this entry is the only
	// record there is.
	ProjectID string                     `json:"projectId,omitempty"`
	Projects  map[string]*ProjectInstall `json:"projects"`
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

// InstallRecord is one install being registered in the global lock.
//
// It is a struct rather than a parameter list because the list had reached ten
// positional strings, four of which were ids and paths that read alike — and the
// eleventh, the publishing project, is the one a project-less install cannot do
// without. A mis-ordered pair there does not fail: it registers the install against
// the wrong owner, or addresses a store that was never built.
type InstallRecord struct {
	ID          string
	Version     string
	Type        ArtifactType
	Name        string
	Description string
	Hash        string
	// CachePath is the local directory the install produced — a clone for a file
	// artifact, a mounted store for AST.
	CachePath string
	// PublisherID is the project that PUBLISHED the artifact, and half of the
	// address of its shared store. Empty means it was published outside any project.
	PublisherID string
	// Owner is who asked for the install: a project id, store.GlobalOwnerKey for an
	// install that belongs to no project, or "__transient__" for a download made to
	// answer one question.
	Owner string
	// OwnerDir is the owner's project directory, and empty for an owner that has
	// none. ValidateProjectDirs keys its staleness check on this, so empty must mean
	// "not a directory to check" rather than "a directory that is gone".
	OwnerDir  string
	LocalPath string
}

func (m *GlobalLockManager) RegisterInstall(rec InstallRecord) (*GlobalArtifact, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	lock, err := m.load()
	if err != nil {
		return nil, err
	}

	key := artifactKey(rec.ID, rec.Version, rec.Type)
	art := lock.Artifacts[key]
	if art == nil {
		art = &GlobalArtifact{
			ID:          rec.ID,
			Name:        rec.Name,
			Description: rec.Description,
			Version:     rec.Version,
			Hash:        rec.Hash,
			Type:        rec.Type,
			CachePath:   rec.CachePath,
			ProjectID:   rec.PublisherID,
			Projects:    make(map[string]*ProjectInstall),
		}
		lock.Artifacts[key] = art
	} else {

		if rec.Name != "" {
			art.Name = rec.Name
		}
		if rec.Description != "" {
			art.Description = rec.Description
		}
		if rec.Hash != "" {
			art.Hash = rec.Hash
		}
		if rec.CachePath != "" {
			art.CachePath = rec.CachePath
		}
		if rec.PublisherID != "" {
			art.ProjectID = rec.PublisherID
		}
	}

	art.Projects[rec.Owner] = &ProjectInstall{
		ProjectDir:  rec.OwnerDir,
		LocalPath:   rec.LocalPath,
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
			// An owner with no directory is not a project whose directory went
			// missing — it is an owner that never had one: store.GlobalOwnerKey for
			// an install that belongs to no project, "__transient__" for a download
			// made to answer a single question. Joining "" with the lockfile name
			// produces a RELATIVE path, which stats against this process's working
			// directory and almost never exists, so the unguarded check deleted
			// exactly the entries that are nobody's to validate.
			if proj.ProjectDir == "" {
				continue
			}
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
