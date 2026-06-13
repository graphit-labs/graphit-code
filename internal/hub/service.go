package hub

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	ideAdapter "github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

type HubService struct {
	Logger   *slog.Logger
	registry *RegistryManager
	tracker  *EventTracker
	lockMgr  *GlobalLockManager
}

func (s *HubService) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

func NewHubService(registry *RegistryManager) *HubService {
	var tracker *EventTracker
	if registry.IsReady() {
		tracker = NewEventTracker(registry.GitStore())
	}
	var lockMgr *GlobalLockManager
	if mgr, err := NewGlobalLockManager(); err == nil {
		lockMgr = mgr
	}
	return &HubService{registry: registry, tracker: tracker, lockMgr: lockMgr}
}

func (s *HubService) ListEntries(typeFilter ArtifactType) []*Entry {
	if s.registry == nil {
		return nil
	}
	return s.registry.ListEntries(typeFilter)
}

type InstallResult struct {
	EntryID  string
	Name     string
	Version  string
	Hash     string
	ArtType  ArtifactType
	IsUpdate bool
}

func (s *HubService) Install(
	ctx context.Context,
	entryID, alias, ide string,
	entryType ArtifactType,
	parentID, projectDir string,
) (*InstallResult, error) {

	reqVersion := ""
	realID := entryID
	if parts := strings.SplitN(entryID, "@", 2); len(parts) == 2 {
		realID, reqVersion = parts[0], parts[1]
	}

	if err := ValidateArtifactID(realID); err != nil {
		return nil, err
	}

	entry := s.registry.GetEntry(realID, entryType)
	if entry == nil {
		return nil, fmt.Errorf("entry %q not found in hub registry — is the registry accessible?", realID)
	}

	constraint, err := ParseVersionConstraint(reqVersion)
	if err != nil {
		return nil, fmt.Errorf("invalid version constraint %q: %w", reqVersion, err)
	}
	var resolvedVersion string
	if constraint.IsLatest() {
		resolvedVersion = entry.Latest
	} else if len(entry.Versions) > 0 {
		resolved, err := ResolveVersion(entry.Versions, constraint)
		if err != nil {
			return nil, fmt.Errorf("no version matching %q for %s: %w", reqVersion, realID, err)
		}
		resolvedVersion = resolved
	} else if constraint.IsExact() {

		resolvedVersion = reqVersion
	} else {
		resolvedVersion = entry.Latest
	}

	localID := alias
	if localID == "" {
		localID = entry.Name
	}
	if localID == "" {
		localID = realID
	}

	artType := entry.Type
	if _, ok := TypeFolderMap[artType]; !ok {
		return nil, fmt.Errorf("unknown artifact type: %q", artType)
	}

	pp := paths.GetPathsForProject(ide, projectDir)

	var cachePath string
	var cloneDir string
	if s.registry.IsReady() {
		var err error
		cloneDir, err = s.registry.EnsureArtifactClone(ctx, artType, realID, resolvedVersion, entry.ProjectID)
		if err != nil {
			return nil, fmt.Errorf("ensuring artifact clone: %w", err)
		}
		cachePath = cloneDir

		switch artType {
		case TypeKnowledge:

			knowledgeDir := filepath.Join(pp.ActiveProjectDir, brand.DotDir(), "knowledge")
			linkPath := filepath.Join(knowledgeDir, entry.ProjectID)
			if err := os.MkdirAll(knowledgeDir, 0o755); err != nil {
				return nil, fmt.Errorf("creating knowledge dir: %w", err)
			}
			if err := paths.SafeCopyDir(cloneDir, linkPath); err != nil {
				return nil, fmt.Errorf("copying knowledge to project: %w", err)
			}

		case TypeAST:
			globalDir := filepath.Join(brand.GlobalDir(), "ast", entry.ProjectID)
			if err := os.MkdirAll(globalDir, 0o755); err != nil {
				return nil, fmt.Errorf("creating global AST dir: %w", err)
			}

			dbPath := filepath.Join(globalDir, "ladybugdb")
			shardCache, err := ast.NewShardCache(cloneDir)
			if err != nil {
				return nil, fmt.Errorf("loading AST shard cache from clone: %w", err)
			}
			if shardCache.Count() > 0 {
				db := ast.NewLadybugDB(ast.LadybugConfig{DBPath: dbPath})
				if err := ast.CreateGraphSchema(ctx, db); err != nil {
					_ = db.Close()
					_ = shardCache.Close()
					return nil, fmt.Errorf("creating AST schema: %w", err)
				}
				var embCache *ast.ShardEmbCache
				if ec, embErr := ast.NewShardEmbCache(cloneDir, shardCache); embErr == nil {
					embCache = ec
					defer func() { _ = embCache.Close() }()
				}
				if err := ast.RebuildFromJSON(ctx, db, shardCache, embCache, "", "", nil); err != nil {
					_ = db.Close()
					_ = shardCache.Close()
					return nil, fmt.Errorf("rebuilding AST DB from cache: %w", err)
				}
				_ = db.Close()
			}
			_ = shardCache.Close()
			cachePath = globalDir

			astDir := filepath.Join(pp.ActiveProjectDir, brand.DotDir(), "ast")
			linkPath := filepath.Join(astDir, entry.ProjectID)
			if err := os.MkdirAll(astDir, 0o755); err != nil {
				return nil, fmt.Errorf("creating ast dir: %w", err)
			}
			if err := paths.SafeSymlink(globalDir, linkPath); err != nil {
				return nil, fmt.Errorf("creating symlink to AST: %w", err)
			}

		case TypeLanguage:

			dotDir := brand.DotDir()
			queriesDir := filepath.Join(pp.ActiveProjectDir, dotDir, "ast", "queries")
			if err := os.MkdirAll(queriesDir, 0o755); err != nil {
				return nil, fmt.Errorf("creating queries dir: %w", err)
			}

			cloneEntries, _ := os.ReadDir(cloneDir)
			for _, ce := range cloneEntries {
				if ce.IsDir() {
					continue
				}
				name := ce.Name()
				src := filepath.Join(cloneDir, name)

				// YAML query definitions.
				if strings.HasSuffix(name, ".yaml") || strings.HasSuffix(name, ".yml") {
					if err := copyFile(src, filepath.Join(queriesDir, name)); err != nil {
						s.log().Warn("installing query yaml", "file", name, "error", err)
					}
					continue
				}

				// Grammar archives: extract platform binary to grammars dir.
				if strings.HasSuffix(name, ".grammar") {
					if err := installGrammarArchive(src, pp.ActiveProjectDir, dotDir); err != nil {
						s.log().Warn("installing grammar archive", "file", name, "error", err)
					}
				}
			}

		case TypeFramework:

			frameworksDir := filepath.Join(pp.ActiveProjectDir, brand.DotDir(), "ast", "frameworks")
			if err := os.MkdirAll(frameworksDir, 0o755); err != nil {
				return nil, fmt.Errorf("creating frameworks dir: %w", err)
			}

			cloneEntries, _ := os.ReadDir(cloneDir)
			for _, ce := range cloneEntries {
				if ce.IsDir() {
					continue
				}
				name := ce.Name()
				if strings.HasSuffix(name, ".yaml") || strings.HasSuffix(name, ".yml") {
					src := filepath.Join(cloneDir, name)
					if err := copyFile(src, filepath.Join(frameworksDir, name)); err != nil {
						s.log().Warn("installing framework yaml", "file", name, "error", err)
					}
				}
			}

		default:

			if ide != "" {
				targetPath, err := ideAdapter.ArtifactTypePath(pp.ActiveProjectDir, ide, string(artType), localID)
				if err == nil {
					if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
						s.log().Warn("creating target dir", "artifact", localID, "error", err)
					}
					fm := ideAdapter.GetFileMode(ide, string(artType))
					if fm == "folder" {
						_ = os.RemoveAll(targetPath)
						if err := copyDir(cloneDir, targetPath); err != nil {
							s.log().Warn("copying artifact to IDE dir", "artifact", localID, "error", err)
						}
					} else {
						srcFile := findCanonicalFile(string(artType), cloneDir)
						if srcFile != "" {
							if err := copyFile(srcFile, targetPath); err != nil {
								s.log().Warn("copying artifact to IDE dir", "artifact", localID, "error", err)
							}
						}
					}
				}
			}
		}
	}

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil {
		return nil, fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return nil, fmt.Errorf("project not initialized — run '%s init' first", brand.BinName())
	}

	memberIDs := make([]string, 0, len(entry.Dependencies))
	for _, dep := range entry.Dependencies {
		memberIDs = append(memberIDs, dep.ID)
	}

	if lf.Artifacts[artType] == nil {
		lf.Artifacts[artType] = make(map[string]*LockfileArtifactMeta)
	}

	installedBy := []string{}
	if parentID != "" {
		installedBy = []string{parentID}
	}

	versionHash := ""
	if entry.Hashes != nil {
		versionHash = entry.Hashes[resolvedVersion]
	}

	lf.Artifacts[artType][realID] = &LockfileArtifactMeta{
		Version:          resolvedVersion,
		Hash:             versionHash,
		InstalledBy:      installedBy,
		Members:          memberIDs,
		ProjectID:        entry.ProjectID,
		Alias:            alias,
		RemoteID:         realID,
		Origin:           "hub",
		RequestedVersion: reqVersion,
	}

	if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
		return nil, fmt.Errorf("saving lockfile: %w", err)
	}

	if s.lockMgr != nil {
		projDir := filepath.Dir(pp.LockFilePath)
		if _, err := s.lockMgr.RegisterInstall(
			realID, resolvedVersion, artType,
			entry.Name, entry.Description, versionHash,
			cachePath, lf.Project.ID, projDir, cloneDir,
		); err != nil {
			s.log().Warn("register install", "id", realID, "version", resolvedVersion, "error", err)
		}
	}

	if err := s.postInstallHook(ctx, artType, realID, cloneDir, pp); err != nil {
		return nil, err
	}

	for _, dep := range entry.Dependencies {

		alreadyInstalled := false
		for _, typeMap := range lf.Artifacts {
			if _, exists := typeMap[dep.ID]; exists {
				alreadyInstalled = true
				break
			}
		}
		if alreadyInstalled {
			continue
		}

		depEntry := s.registry.GetEntry(dep.ID, dep.Type)
		if depEntry == nil {
			continue
		}

		depIDVersioned := dep.ID
		if dep.Version != "" {
			depIDVersioned = dep.ID + "@" + dep.Version
		}

		if _, err := s.Install(ctx, depIDVersioned, "", ide, dep.Type, realID, projectDir); err != nil {
			s.log().Warn("install dependency", "dependency", dep.ID, "parent", realID, "error", err)
		}
	}

	result := &InstallResult{
		EntryID: realID,
		Name:    localID,
		Version: resolvedVersion,
		Hash:    versionHash,
		ArtType: artType,
	}

	s.tracker.TrackEvent("artifact.install", "",
		map[string]string{"type": string(artType), "version": resolvedVersion},
		map[string]string{"ide": ide})

	return result, nil
}

func (s *HubService) RecordPublish(
	ctx context.Context,
	entryID string,
	artType ArtifactType,
	version string,
	ide, projectDir string,
) error {
	pp := paths.GetPathsForProject(ide, projectDir)

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil {
		return fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return fmt.Errorf("project not initialized — run '%s init' first", brand.BinName())
	}

	if lf.Artifacts[artType] == nil {
		lf.Artifacts[artType] = make(map[string]*LockfileArtifactMeta)
	}

	existing := lf.Artifacts[artType][entryID]
	if existing != nil {
		existing.Version = version
		if existing.Origin == "" || existing.Origin == "local" {
			existing.Origin = "publish"
		}
		existing.RemoteID = entryID
	} else {
		lf.Artifacts[artType][entryID] = &LockfileArtifactMeta{
			Version:  version,
			Origin:   "publish",
			RemoteID: entryID,
		}
	}

	if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
		return err
	}

	s.recordPublishInGlobalLock(entryID, artType, version, lf.Project.ID, filepath.Dir(pp.LockFilePath))
	return nil
}

func (s *HubService) recordPublishInGlobalLock(
	entryID string,
	artType ArtifactType,
	version string,
	projectID, projectDir string,
) {
	if s.lockMgr == nil {
		return
	}

	name := entryID
	description := ""
	versionHash := ""
	if s.registry != nil && s.registry.IsReady() {
		if entry := s.registry.GetEntry(entryID, artType); entry != nil {
			if entry.Name != "" {
				name = entry.Name
			}
			description = entry.Description
			if entry.Hashes != nil {
				versionHash = entry.Hashes[version]
			}
		}
	}

	if _, err := s.lockMgr.RegisterInstall(
		entryID, version, artType,
		name, description, versionHash,
		"", projectID, projectDir, "",
	); err != nil {
		s.log().Warn("register publish in global lock", "id", entryID, "version", version, "error", err)
	}
}

func (s *HubService) Uninstall(
	ctx context.Context,
	entryID string,
	entryType ArtifactType,
	forceRoot bool,
	ide, projectDir string,
) error {
	pp := paths.GetPathsForProject(ide, projectDir)

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		return nil
	}

	artType := entryType
	var meta *LockfileArtifactMeta

	if artType != "" {
		meta = lf.Artifacts[artType][entryID]
	} else {
		for t, typeMap := range lf.Artifacts {
			if m, ok := typeMap[entryID]; ok {
				artType = t
				meta = m
				break
			}
		}
	}

	if meta == nil {
		if !forceRoot {
			return fmt.Errorf("entry %q not found in hub tracking", entryID)
		}
		return nil
	}

	for _, memberID := range meta.Members {
		if err := s.Uninstall(ctx, memberID, "", false, ide, projectDir); err != nil {
			s.log().Warn("uninstall member", "member", memberID, "parent", entryID, "error", err)
		}
	}

	if !forceRoot && len(meta.InstalledBy) > 0 {

		updated := meta.InstalledBy[:len(meta.InstalledBy)-1]
		if len(updated) > 0 {
			meta.InstalledBy = updated
			return SaveLockfile(pp.LockFilePath, lf)
		}
	}

	if err := s.preUninstallHook(ctx, artType, entryID, meta, pp); err != nil {
		s.log().Warn("pre-uninstall hook", "type", artType, "id", entryID, "error", err)
	}

	if !ideIndependentTypes[artType] && ide != "" {
		if targetPath, err := ideArtifactPath(pp.ActiveProjectDir, ide, string(artType), entryID); err == nil {
			if info, err := os.Lstat(targetPath); err == nil {
				if info.IsDir() {
					_ = os.RemoveAll(targetPath)
				} else {
					_ = os.Remove(targetPath)
				}
			}
		}
	}

	delete(lf.Artifacts[artType], entryID)
	if len(lf.Artifacts[artType]) == 0 {
		delete(lf.Artifacts, artType)
	}

	if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
		return err
	}

	s.tracker.TrackEvent("artifact.uninstall", "",
		map[string]string{"type": string(artType), "version": meta.Version},
		map[string]string{"ide": ide})

	if s.lockMgr != nil && meta.Version != "" {
		projectDir := filepath.Dir(pp.LockFilePath)
		projectID := ""
		if lf2, err := LoadLockfile(pp.LockFilePath); err == nil && lf2 != nil {
			projectID = lf2.Project.ID
		}
		if projectID == "" {
			projectID = projectDir
		}
		orphaned, gcErr := s.lockMgr.RegisterUninstall(entryID, meta.Version, artType, projectID)
		if gcErr != nil {
			s.log().Warn("deregister", "id", entryID, "version", meta.Version, "error", gcErr)
		}
		if orphaned {

			if _, gcErr := s.lockMgr.GCOrphans(); gcErr != nil {
				s.log().Warn("GC orphans", "after", entryID, "error", gcErr)
			}
		}
	}

	return nil
}

func (s *HubService) UpdateOne(ctx context.Context, entryID string, entryType ArtifactType, ide, projectDir string) error {
	pp := paths.GetPathsForProject(ide, projectDir)

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		return fmt.Errorf("lockfile not found — run '%s init' first", brand.BinName())
	}

	artType := entryType
	resolvedID := entryID
	var meta *LockfileArtifactMeta

	if artType != "" {

		meta = lf.Artifacts[artType][entryID]
		if meta == nil {
			for id, m := range lf.Artifacts[artType] {
				if m.Alias == entryID {
					meta = m
					resolvedID = id
					break
				}
			}
		}
	} else {

		for t, typeMap := range lf.Artifacts {
			if m, ok := typeMap[entryID]; ok {
				artType = t
				meta = m
				break
			}
		}
		if meta == nil {
			for t, typeMap := range lf.Artifacts {
				for id, m := range typeMap {
					if m.Alias == entryID {
						artType = t
						meta = m
						resolvedID = id
						break
					}
				}
				if meta != nil {
					break
				}
			}
		}
	}

	if meta == nil {
		return fmt.Errorf("%q (%s) is not installed", entryID, entryType)
	}
	entryType = artType

	remoteID := meta.RemoteID
	if remoteID == "" {
		remoteID = resolvedID
	}

	entry := s.registry.GetEntry(remoteID, entryType)
	if entry == nil {
		return fmt.Errorf("%q not found in registry", remoteID)
	}

	if entry.Latest == "" {
		return nil
	}

	constraint, _ := ParseVersionConstraint(meta.RequestedVersion)
	targetVersion := entry.Latest
	if !constraint.IsLatest() && len(entry.Versions) > 0 {
		if resolved, err := ResolveVersion(entry.Versions, constraint); err == nil {
			targetVersion = resolved
		} else {
			return nil
		}
	}

	if meta.Version != targetVersion {

		if err := s.Uninstall(ctx, resolvedID, entryType, true, ide, projectDir); err != nil {
			return err
		}

		newID := remoteID
		if meta.RequestedVersion != "" {
			newID += "@" + meta.RequestedVersion
		} else {
			newID += "@" + targetVersion
		}
		_, err = s.Install(ctx, newID, meta.Alias, ide, entryType, "", projectDir)
		return err
	}

	registryHash := ""
	if entry.Hashes != nil {
		registryHash = entry.Hashes[meta.Version]
	}

	localHash := ""
	installPath := resolveArtifactPath(meta, entryType, resolvedID, pp)
	if installPath != "" {
		if resolved, err := filepath.EvalSymlinks(installPath); err == nil {
			installPath = resolved
		}
		if h, err := HashPath(installPath); err == nil {
			localHash = h
		}
	}

	if localHash != "" && strings.EqualFold(localHash, registryHash) {

		if meta.Hash != localHash {
			meta.Hash = localHash
			if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
				s.log().Warn("update hash", "id", resolvedID, "error", err)
			}
		}
		return nil
	}

	if err := s.Uninstall(ctx, resolvedID, entryType, true, ide, projectDir); err != nil {
		return err
	}
	newID := remoteID
	if meta.RequestedVersion != "" {
		newID += "@" + meta.RequestedVersion
	} else {
		newID += "@" + targetVersion
	}
	_, err = s.Install(ctx, newID, meta.Alias, ide, entryType, "", projectDir)
	return err
}

func (s *HubService) UpdateAll(ctx context.Context, ide, projectDir string) map[string]error {
	pp := paths.GetPathsForProject(ide, projectDir)
	result := make(map[string]error)

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		return result
	}

	for artType, typeMap := range lf.Artifacts {
		for artID, meta := range typeMap {
			if meta.RemoteID == "" {
				continue
			}
			result[artID] = s.UpdateOne(ctx, artID, artType, ide, projectDir)
		}
	}
	return result
}

func (s *HubService) UninstallAll(ctx context.Context, ide, projectDir string) error {
	pp := paths.GetPathsForProject(ide, projectDir)

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		return nil
	}

	for artType, typeMap := range lf.Artifacts {
		for artID := range typeMap {
			if err := s.Uninstall(ctx, artID, artType, true, ide, projectDir); err != nil {
				s.log().Warn("uninstall-all", "type", artType, "id", artID, "error", err)
			}
		}
	}

	return os.Remove(pp.LockFilePath)
}

func (s *HubService) postInstallHook(ctx context.Context, artType ArtifactType, id, dir string, pp *paths.ProjectPaths) error {

	return nil
}

func (s *HubService) preUninstallHook(ctx context.Context, artType ArtifactType, id string, meta *LockfileArtifactMeta, pp *paths.ProjectPaths) error {
	switch artType {
	case TypeKnowledge:

		linkName := id
		if meta != nil && meta.ProjectID != "" {
			linkName = meta.ProjectID
		}
		target := filepath.Join(pp.ActiveProjectDir, brand.DotDir(), "knowledge", linkName)
		if info, err := os.Lstat(target); err == nil {
			if info.Mode()&os.ModeSymlink != 0 {
				return os.Remove(target)
			}
			return os.RemoveAll(target)
		}
		return nil
	case TypeAST:

		linkName := id
		if meta != nil && meta.ProjectID != "" {
			linkName = meta.ProjectID
		}
		target := filepath.Join(pp.ActiveProjectDir, brand.DotDir(), "ast", linkName)
		if info, err := os.Lstat(target); err == nil {
			if info.Mode()&os.ModeSymlink != 0 {
				return os.Remove(target)
			}
			return os.RemoveAll(target)
		}
		return nil
	case TypeLanguage:

		dotDir := brand.DotDir()
		queriesDir := filepath.Join(pp.ActiveProjectDir, dotDir, "ast", "queries")

		cloneDir := resolveArtifactPath(meta, artType, id, pp)
		if cloneDir == "" {
			return nil
		}
		cloneEntries, _ := os.ReadDir(cloneDir)
		for _, ce := range cloneEntries {
			if ce.IsDir() {
				continue
			}
			name := ce.Name()
			if strings.HasSuffix(name, ".yaml") || strings.HasSuffix(name, ".yml") {
				_ = os.Remove(filepath.Join(queriesDir, name))
			}
		}
		// Also remove grammar binaries.
		uninstallGrammarFiles(cloneDir, pp.ActiveProjectDir, dotDir)
		return nil
	case TypeFramework:

		frameworksDir := filepath.Join(pp.ActiveProjectDir, brand.DotDir(), "ast", "frameworks")

		cloneDir := resolveArtifactPath(meta, artType, id, pp)
		if cloneDir == "" {
			return nil
		}
		cloneEntries, _ := os.ReadDir(cloneDir)
		for _, ce := range cloneEntries {
			if ce.IsDir() {
				continue
			}
			name := ce.Name()
			if strings.HasSuffix(name, ".yaml") || strings.HasSuffix(name, ".yml") {
				_ = os.Remove(filepath.Join(frameworksDir, name))
			}
		}
		return nil
	}
	return nil
}

type LinkResult struct {
	ArtifactID string
	ArtType    ArtifactType
	SourcePath string
	Links      []string
}

func (s *HubService) Link(
	ctx context.Context,
	artifactName, sourcePath, ide string,
	artType ArtifactType,
	projectDir string,
) (*LinkResult, error) {
	if err := ValidateArtifactID(artifactName); err != nil {
		return nil, err
	}
	if artType == "" {
		return nil, fmt.Errorf("--type is required for link")
	}

	absSource, err := filepath.Abs(sourcePath)
	if err != nil {
		return nil, fmt.Errorf("resolving source path: %w", err)
	}
	if info, err := os.Stat(absSource); err != nil || !info.IsDir() {
		return nil, fmt.Errorf("source path %q is not a directory", absSource)
	}

	pp := paths.GetPathsForProject(ide, projectDir)
	dotDir := brand.DotDir()
	result := &LinkResult{
		ArtifactID: artifactName,
		ArtType:    artType,
		SourcePath: absSource,
	}

	switch artType {
	case TypeAST:

		sourceAST := filepath.Join(absSource, dotDir, "ast", "project")
		if _, err := os.Stat(sourceAST); err != nil {
			return nil, fmt.Errorf("source AST not found at %s", sourceAST)
		}
		astDir := filepath.Join(pp.ActiveProjectDir, dotDir, "ast")
		linkPath := filepath.Join(astDir, artifactName)
		if err := os.MkdirAll(astDir, 0o755); err != nil {
			return nil, err
		}
		if err := paths.SafeSymlink(sourceAST, linkPath); err != nil {
			return nil, fmt.Errorf("creating AST symlink: %w", err)
		}
		result.Links = append(result.Links, linkPath+" → "+sourceAST)

	case TypeKnowledge:

		sourceKnowledge := filepath.Join(absSource, dotDir, "knowledge", "project")
		if _, err := os.Stat(sourceKnowledge); err != nil {
			return nil, fmt.Errorf("source knowledge not found at %s", sourceKnowledge)
		}
		knDir := filepath.Join(pp.ActiveProjectDir, dotDir, "knowledge")
		linkPath := filepath.Join(knDir, artifactName)
		if err := os.MkdirAll(knDir, 0o755); err != nil {
			return nil, err
		}
		if err := paths.SafeCopyDir(sourceKnowledge, linkPath); err != nil {
			return nil, fmt.Errorf("copying knowledge to project: %w", err)
		}
		result.Links = append(result.Links, "copied "+sourceKnowledge+" → "+linkPath)

	case TypeMCP:

		mcpDir := filepath.Join(absSource, dotDir, TypeFolderMap[TypeMCP], artifactName)
		if _, err := os.Stat(mcpDir); err != nil {
			return nil, fmt.Errorf("source MCP artifact not found at %s", mcpDir)
		}
		adapter := ideAdapter.GetAdapter(ide)
		if adapter == nil {
			return nil, fmt.Errorf("unknown IDE: %s", ide)
		}
		installed := map[string]map[string]string{
			artifactName: {"type": "mcp", "path": mcpDir},
		}
		lf, loadErr := LoadLockfile(pp.LockFilePath)
		projectID := ""
		if loadErr == nil && lf != nil {
			projectID = lf.Project.ID
		}
		if err := adapter.Sync(installed, pp, projectID); err != nil {
			return nil, fmt.Errorf("installing MCP config: %w", err)
		}
		result.Links = append(result.Links, "MCP config installed from "+mcpDir)

	case TypeLanguage:

		sourceQueries := filepath.Join(absSource, dotDir, "ast", "queries")
		queriesDir := filepath.Join(pp.ActiveProjectDir, dotDir, "ast", "queries")
		_ = os.MkdirAll(queriesDir, 0o755)

		if entries, err := os.ReadDir(sourceQueries); err == nil {
			for _, e := range entries {
				if !e.IsDir() && (strings.HasSuffix(e.Name(), ".yaml") || strings.HasSuffix(e.Name(), ".yml")) {
					src := filepath.Join(sourceQueries, e.Name())
					dst := filepath.Join(queriesDir, e.Name())
					if err := copyFile(src, dst); err == nil {
						result.Links = append(result.Links, "copied "+src+" → "+dst)
					}
				}
			}
		}

	case TypeFramework:

		sourceFrameworks := filepath.Join(absSource, dotDir, "ast", "frameworks")
		frameworksDir := filepath.Join(pp.ActiveProjectDir, dotDir, "ast", "frameworks")
		_ = os.MkdirAll(frameworksDir, 0o755)

		if entries, err := os.ReadDir(sourceFrameworks); err == nil {
			for _, e := range entries {
				if !e.IsDir() && (strings.HasSuffix(e.Name(), ".yaml") || strings.HasSuffix(e.Name(), ".yml")) {
					src := filepath.Join(sourceFrameworks, e.Name())
					dst := filepath.Join(frameworksDir, e.Name())
					if err := copyFile(src, dst); err == nil {
						result.Links = append(result.Links, "copied "+src+" → "+dst)
					}
				}
			}
		}

	default:

		sourcePath, err := ideArtifactPath(absSource, ide, string(artType), artifactName)
		if err != nil {
			return nil, fmt.Errorf("resolving source artifact path: %w", err)
		}
		if _, err := os.Stat(sourcePath); err != nil {
			return nil, fmt.Errorf("source artifact not found at %s", sourcePath)
		}
		targetPath, err := ideArtifactPath(pp.ActiveProjectDir, ide, string(artType), artifactName)
		if err != nil {
			return nil, fmt.Errorf("resolving target artifact path: %w", err)
		}
		if err := os.MkdirAll(filepath.Dir(targetPath), 0o755); err != nil {
			return nil, err
		}
		if err := paths.SafeSymlink(sourcePath, targetPath); err != nil {
			return nil, fmt.Errorf("creating %s symlink: %w", artType, err)
		}
		result.Links = append(result.Links, targetPath+" → "+sourcePath)
	}

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil {
		return nil, fmt.Errorf("reading lockfile: %w", err)
	}
	if lf == nil {
		return nil, fmt.Errorf("project not initialized — run '%s init' first", brand.BinName())
	}

	if lf.Artifacts[artType] == nil {
		lf.Artifacts[artType] = make(map[string]*LockfileArtifactMeta)
	}
	lf.Artifacts[artType][artifactName] = &LockfileArtifactMeta{
		Version:    "local",
		Origin:     "link",
		LinkSource: absSource,
		RemoteID:   artifactName,
	}

	if err := SaveLockfile(pp.LockFilePath, lf); err != nil {
		return nil, fmt.Errorf("saving lockfile: %w", err)
	}

	return result, nil
}

func (s *HubService) Unlink(
	ctx context.Context,
	artifactName, ide string,
	artType ArtifactType,
	projectDir string,
) error {
	if artType == "" {
		return fmt.Errorf("--type is required for unlink")
	}

	pp := paths.GetPathsForProject(ide, projectDir)
	dotDir := brand.DotDir()

	lf, err := LoadLockfile(pp.LockFilePath)
	if err != nil || lf == nil {
		return fmt.Errorf("lockfile not found")
	}

	meta := lf.Artifacts[artType][artifactName]
	if meta == nil {
		return fmt.Errorf("artifact %q (type=%s) not found in lockfile", artifactName, artType)
	}
	if meta.Origin != "link" {
		return fmt.Errorf("%q is not a linked artifact (origin=%s) — use 'uninstall' instead", artifactName, meta.Origin)
	}

	switch artType {
	case TypeAST:
		_ = os.Remove(filepath.Join(pp.ActiveProjectDir, dotDir, "ast", artifactName))

	case TypeKnowledge:
		_ = os.Remove(filepath.Join(pp.ActiveProjectDir, dotDir, "knowledge", artifactName))

	default:
		if targetPath, err := ideArtifactPath(pp.ActiveProjectDir, ide, string(artType), artifactName); err == nil {
			_ = os.Remove(targetPath)
		}
	}

	delete(lf.Artifacts[artType], artifactName)
	if len(lf.Artifacts[artType]) == 0 {
		delete(lf.Artifacts, artType)
	}

	return SaveLockfile(pp.LockFilePath, lf)
}

func ideArtifactPath(projectDir, ide, artifactType, artifactName string) (string, error) {
	return ideAdapter.ArtifactTypePath(projectDir, ide, artifactType, artifactName)
}

func (s *HubService) EnsureKnowledgeAvailable(ctx context.Context, artifactID string) (string, error) {

	reqVersion := ""
	realID := artifactID
	if parts := strings.SplitN(artifactID, "@", 2); len(parts) == 2 {
		realID, reqVersion = parts[0], parts[1]
	}

	entry := s.registry.GetEntry(realID, TypeKnowledge)
	if entry == nil {
		return "", fmt.Errorf("knowledge artifact %q not found in hub registry", realID)
	}

	constraint, err := ParseVersionConstraint(reqVersion)
	if err != nil {
		return "", fmt.Errorf("invalid version constraint %q: %w", reqVersion, err)
	}
	var resolvedVersion string
	if constraint.IsLatest() {
		resolvedVersion = entry.Latest
	} else if len(entry.Versions) > 0 {
		resolved, err := ResolveVersion(entry.Versions, constraint)
		if err != nil {
			return "", fmt.Errorf("no version matching %q for %s: %w", reqVersion, realID, err)
		}
		resolvedVersion = resolved
	} else if constraint.IsExact() {
		resolvedVersion = reqVersion
	} else {
		resolvedVersion = entry.Latest
	}

	if !s.registry.IsReady() {
		return "", fmt.Errorf("hub registry not available — cannot download knowledge artifact %q", realID)
	}

	cloneDir, err := s.registry.EnsureArtifactClone(ctx, TypeKnowledge, realID, resolvedVersion, entry.ProjectID)
	if err != nil {
		return "", fmt.Errorf("downloading knowledge artifact %s@%s: %w", realID, resolvedVersion, err)
	}

	if s.lockMgr != nil {
		versionHash := ""
		if entry.Hashes != nil {
			versionHash = entry.Hashes[resolvedVersion]
		}
		if _, err := s.lockMgr.RegisterInstall(
			realID, resolvedVersion, TypeKnowledge,
			entry.Name, entry.Description, versionHash,
			cloneDir, "__transient__", "", cloneDir,
		); err != nil {
			s.log().Warn("register transient install", "id", realID, "version", resolvedVersion, "error", err)
		}
	}

	wikiDir := filepath.Join(cloneDir, "wiki")
	if info, err := os.Stat(wikiDir); err == nil && info.IsDir() {
		return wikiDir, nil
	}
	return cloneDir, nil
}

func findCanonicalFile(artType, dir string) string {
	canonical := map[string]string{
		"rule": "RULE.md", "command": "COMMAND.md",
		"agent": "AGENT.md", "skill": "SKILL.md",
	}
	if name, ok := canonical[artType]; ok {
		path := filepath.Join(dir, name)
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		return ""
	}
	for _, e := range entries {
		if !e.IsDir() {
			return filepath.Join(dir, e.Name())
		}
	}
	return ""
}

func resolveArtifactPath(meta *LockfileArtifactMeta, artType ArtifactType, artID string, pp *paths.ProjectPaths) string {
	if meta.LinkSource != "" {
		return meta.LinkSource
	}

	hubDir, err := config.HubRepoDirPath()
	if err == nil {
		gs := &GitStore{repoDir: hubDir, cacheBase: hubDir + "-cache"}
		remoteID := meta.RemoteID
		if remoteID == "" {
			remoteID = artID
		}
		return gs.ArtifactCloneDir(artType, remoteID, meta.Version, meta.ProjectID)
	}

	folder := TypeFolderMap[artType]
	return filepath.Join(pp.ResourcesDir, folder, artID, meta.Version)
}
