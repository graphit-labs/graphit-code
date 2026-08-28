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
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type HubService struct {
	Logger   *slog.Logger
	registry *RegistryManager
	tracker  *EventTracker
	lockMgr  *GlobalLockManager
}

func (s *HubService) log() *slog.Logger { return slogutil.Resolve(s.Logger) }

// NewUntrackedHubService builds a service that installs artifacts without recording
// anything about the project that asked for them.
//
// Install normally does two things beyond placing files: it registers the install in
// the global lock, and it publishes an artifact.install event into the Hub's git
// store. Both are records *of a project*, and the live search's ephemeral project is
// not one — it is created for a single search and deleted with it, so either record
// would outlive its subject and be keyed to a project ID that resolves to nothing.
//
// Those two are the only difference. Resolving the version, cloning, placing the
// artifact, writing the project's own lockfile, installing dependencies: all of it
// happens exactly as for a real project, because the ephemeral project has to behave
// exactly like one.
//
// Suppression works by leaving the two collaborators nil rather than by a flag:
// RegisterInstall is already guarded by a nil check and TrackEvent already tolerates
// a nil receiver, because a service built without a reachable registry has neither.
func NewUntrackedHubService(registry *RegistryManager) *HubService {
	return &HubService{registry: registry}
}

func NewHubService(registry *RegistryManager) *HubService {
	var tracker *EventTracker
	if registry.IsReady() {
		tracker = NewEventTracker(registry.Store())
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

// GetEntry looks up one registry entry by ID and type, or nil when the registry
// does not have it.
func (s *HubService) GetEntry(id string, entryType ArtifactType) *Entry {
	if s.registry == nil {
		return nil
	}
	return s.registry.GetEntry(id, entryType)
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

	// NOTHING MOUNTABLE IS TRANSFERRED ANY MORE. Both artifact families are read where they were
	// published: a knowledge artifact is a search index and mounts as one, and an AST artifact is
	// an icebug graph plus a search index, both of which the engines open over `s3://`.
	//
	// Decided BEFORE the clone, because the clone is the transfer. The rest of the install still
	// runs — the lockfile entry, the dependencies, the telemetry — and the lockfile entry is what
	// makes the mount resolvable later: the location is DERIVED from it rather than stored. See
	// internal/hub/mount.go.
	//
	// The graph half carries known gaps, ACCEPTED rather than hidden: multi-hop traversal over a
	// mounted graph is weaker than over a native one, and a relationship table holds one CSR so
	// every label is folded into `Entity` with the label as a column. Both are format limits,
	// measured and recorded in docs/tasks/hub-em-s3-icebug-e-lancedb.md, and stated again in
	// internal/ast/icebug_transfer.go where a caller will meet them.
	mountArtifact := s.registry.MountsArtifact(artType)

	var cachePath string
	var cloneDir string
	// The mounted path still has work to do for AST: the local CATALOG has to exist, or there is
	// nothing for a query to resolve against. It is the DDL and nothing else — no graph bytes —
	// which is why it lives here, outside the clone branch, instead of looking like part of it.
	if mountArtifact && artType == TypeAST {
		contextID := ast.HubContextID(entry.ProjectID, realID)
		storeDir, err := s.ensureASTStore(ctx, "", contextID, resolvedVersion,
			artifactRef{ID: realID, ProjectID: entry.ProjectID})
		if err != nil {
			return nil, err
		}
		cachePath = storeDir
	}
	if s.registry.IsReady() && !mountArtifact {
		var err error
		cloneDir, err = s.registry.EnsureArtifactClone(ctx, artType, realID, resolvedVersion, entry.ProjectID)
		if err != nil {
			return nil, fmt.Errorf("ensuring artifact clone: %w", err)
		}
		cachePath = cloneDir

		switch artType {
		case TypeKnowledge:
			// One copy per machine, in the global wiki root, shared by every project
			// that installs the artifact. The project records that it has it; it does
			// not carry the pages, which an agent reads through the wiki MCP tools
			// rather than off disk.
			//
			// The artifact is placed and then INDEXED, never recompiled: it was
			// published compiled, shards and embedding vectors included, so running
			// the generator again would re-derive pages from sources that need not
			// have travelled and re-run the embedding model over text whose vectors
			// are already in hand.
			contextName := store.ContextNameFor(realID, entry.ProjectID)
			// VERSIONED, like the AST store beside it. It used to land in the
			// unversioned context directory, so two projects pinned to different
			// versions of the same artifact shared one copy and the last install
			// silently won — while both lockfiles recorded a version nothing enforced.
			//
			// internal/knowledge cannot be reached from here — it imports this
			// package for its rule installation — so the two primitives it would
			// have offered are used directly. They live in internal/wiki, which is
			// neutral to both.
			ctxDir, err := wiki.ResetDir(store.KnowledgeHubDir(contextName, resolvedVersion))
			if err != nil {
				return nil, err
			}
			if err := paths.SafeCopyDir(publishedWikiDir(cloneDir), ctxDir); err != nil {
				return nil, fmt.Errorf("copying knowledge to the global wiki: %w", err)
			}
			// An artifact that carries its tables is LOADED, not indexed: the publisher
			// already chunked, embedded and assembled, so the consumer copies rows in
			// and builds only the indexes, which are engine structures and cannot
			// travel. Artifacts published before that carry shards and still take the
			// path below, which is why both exist.
			if wiki.HasBundle(ctxDir) {
				if _, err := wiki.ImportFromParquet(ctx, ctxDir, wiki.BundlePath(ctxDir)); err != nil {
					return nil, fmt.Errorf("loading knowledge context %q: %w", contextName, err)
				}
			} else if _, err := wiki.BuildDBFromCache(ctx, ctxDir); err != nil {
				return nil, fmt.Errorf("indexing knowledge context %q: %w", contextName, err)
			}
			// Nothing is registered here. The lockfile entry written after this switch
			// records the claim, with the version that keys the directory above — the
			// second registry that used to be written at this point is gone.

		case TypeAST:
			// The store is shared between every project pinned to this version and
			// nothing is placed in the project itself: resolution reads the
			// lockfile entry written below, and an AST store is only ever reached
			// through this binary, never opened as files by an agent.
			contextID := ast.HubContextID(entry.ProjectID, realID)
			storeDir, err := s.ensureASTStore(ctx, cloneDir, contextID, resolvedVersion,
				artifactRef{ID: realID, ProjectID: entry.ProjectID})
			if err != nil {
				return nil, err
			}
			cachePath = storeDir

		case TypeLanguage:

			globalDir := brand.GlobalDir()
			if globalDir == "" {
				return nil, fmt.Errorf("cannot determine global dir for language artifact")
			}
			queriesDir := filepath.Join(globalDir, "ast", "queries")
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

				// Grammar archives: extract platform binary to global grammars dir.
				if strings.HasSuffix(name, ".grammar") {
					if err := installGrammarArchive(src, globalDir, ""); err != nil {
						s.log().Warn("installing grammar archive", "file", name, "error", err)
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

	if mountArtifact {
		s.log().Info("context mounted, not downloaded",
			"type", string(artType), "artifact", realID, "version", resolvedVersion,
			"read_from", "object storage")
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
			// For language artifacts, clean up global files only when orphaned.
			if artType == TypeLanguage {
				s.cleanupGlobalLanguageFiles(meta, entryID, pp)
			}
			// An AST store is shared by every project on its version, so an
			// uninstall removes nothing until the last of them lets go.
			if artType == TypeAST {
				s.cleanupSharedASTStore(meta, entryID)
			}
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

// publishedWikiDir locates the compiled wiki inside a downloaded knowledge artifact.
//
// An artifact published by `knowledge export` carries it under `wiki/`, next to
// whatever else its author chose to include. One submitted with `hub submit
// --local-path <a wiki dir>` IS the wiki. Both shapes are legitimate, and telling
// them apart by looking for the index is more honest than demanding a convention the
// publisher never agreed to.
func publishedWikiDir(cloneDir string) string {
	sub := filepath.Join(cloneDir, "wiki")
	if _, err := os.Stat(filepath.Join(sub, "index.md")); err == nil {
		return sub
	}
	if _, err := os.Stat(filepath.Join(sub, "shards")); err == nil {
		return sub
	}
	return cloneDir
}

func (s *HubService) postInstallHook(ctx context.Context, artType ArtifactType, id, dir string, pp *paths.ProjectPaths) error {

	return nil
}

func (s *HubService) preUninstallHook(ctx context.Context, artType ArtifactType, id string, meta *LockfileArtifactMeta, pp *paths.ProjectPaths) error {
	switch artType {
	case TypeKnowledge:
		// The wiki is shared by every project that installed the artifact, so an
		// uninstall drops this project's claim and nothing else. The directory is
		// collected when the global lock reports the artifact orphaned.
		name := id
		if meta != nil && meta.ProjectID != "" {
			name = meta.ProjectID
		}
		_ = store.RemoveContext(pp.ActiveProjectDir, store.KindKnowledge, name)
		return nil
	case TypeAST:
		// Same reasoning: the store is shared across every project pinned to this
		// version and is removed only when the last of them lets go — handled in
		// Uninstall once RegisterUninstall reports the artifact orphaned. The
		// project's claim lives in its lockfile, which the caller is already
		// rewriting, so there is nothing project-local left to clean up.
		return nil
	case TypeLanguage:
		// Language artifacts live in global dirs and are shared across projects.
		// File cleanup happens only when orphaned (no project references remain),
		// handled in Uninstall after RegisterUninstall confirms orphaned status.
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
		// Linking a sibling project's graph is now a pointer, not a symlink: the
		// sibling's store is global and already built, so this project records where
		// it is and queries it in place. Nothing is copied and nothing is linked,
		// which also means the link never goes stale when the sibling reindexes.
		sourceAST := filepath.Join(store.ASTProjectIcebugDir(absSource), ast.IcebugSchemaFile)
		if _, err := os.Stat(sourceAST); err != nil {
			return nil, fmt.Errorf("source AST not found at %s — index the source project first", sourceAST)
		}
		// The sibling's DIRECTORY is what gets recorded; its store is derived on every
		// read. sourceAST above is only the existence check.
		if err := ast.LinkImportedContext(pp.ActiveProjectDir, artifactName, absSource); err != nil {
			return nil, fmt.Errorf("registering linked AST context: %w", err)
		}
		result.Links = append(result.Links, artifactName+" → "+sourceAST)

	case TypeKnowledge:

		sourceKnowledge := store.KnowledgeProjectDir(absSource)
		if _, err := os.Stat(sourceKnowledge); err != nil {
			return nil, fmt.Errorf("source knowledge not found at %s — index the source project first", sourceKnowledge)
		}
		if err := store.AddContext(pp.ActiveProjectDir, store.KindKnowledge, store.ContextRecord{
			Name:       artifactName,
			SourcePath: absSource,
			Origin:     projectlock.OriginLink,
		}); err != nil {
			return nil, fmt.Errorf("registering linked knowledge context: %w", err)
		}
		result.Links = append(result.Links, artifactName+" → "+sourceKnowledge)

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

		globalDir := brand.GlobalDir()
		if globalDir == "" {
			break
		}
		// Where that project keeps its grammars is its own configuration
		// (ast.queries_dir), not a fixed path under the brand directory.
		sourceQueries := ast.ProjectQueriesDir(absSource)
		queriesDir := filepath.Join(globalDir, "ast", "queries")
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
		_ = store.RemoveContext(pp.ActiveProjectDir, store.KindAST, artifactName)

	case TypeKnowledge:
		_ = store.RemoveContext(pp.ActiveProjectDir, store.KindKnowledge, artifactName)

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

	resolvedVersion, err := resolveEntryVersion(entry, reqVersion)
	if err != nil {
		return "", err
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
		remoteID := meta.RemoteID
		if remoteID == "" {
			remoteID = artID
		}
		return ArtifactCacheDirIn(hubDir, artType, remoteID, meta.Version, meta.ProjectID)
	}

	folder := TypeFolderMap[artType]
	return filepath.Join(pp.ResourcesDir, folder, artID, meta.Version)
}

// cleanupGlobalLanguageFiles removes language artifact files (YAMLs and grammar
// binaries) from the global directories. Called only when the artifact is orphaned
// — i.e., no other project references it in the global lock.
func (s *HubService) cleanupGlobalLanguageFiles(meta *LockfileArtifactMeta, id string, pp *paths.ProjectPaths) {
	globalDir := brand.GlobalDir()
	if globalDir == "" {
		return
	}

	cloneDir := resolveArtifactPath(meta, TypeLanguage, id, pp)
	if cloneDir == "" {
		return
	}

	queriesDir := filepath.Join(globalDir, "ast", "queries")
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
	uninstallGrammarFiles(cloneDir, globalDir, "")
}
