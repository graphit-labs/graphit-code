package hub

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	ideAdapter "github.com/graphit-labs/graphit-code/internal/hub/adapters/ide"
	"github.com/graphit-labs/graphit-code/internal/lancestore"
	"github.com/graphit-labs/graphit-code/internal/paths"
	"github.com/graphit-labs/graphit-code/internal/projectlock"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
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

// globallyInstalled lists the artifacts installed with no project — the global lock's
// equivalent of reading a project's lockfile.
//
// The reserved owner is what distinguishes them. An artifact whose only owners are real
// projects is in the global lock because that is where every install is recorded, not
// because anyone installed it globally.
func (s *HubService) globallyInstalled() []*GlobalArtifact {
	if s.lockMgr == nil {
		return nil
	}
	arts, err := s.lockMgr.ListInstalledInProject(store.GlobalOwnerKey)
	if err != nil {
		s.log().Warn("listing global installs", "error", err)
		return nil
	}
	return arts
}

// GlobalInstalls is globallyInstalled for callers outside this package — the MCP layer
// answering "what can I reach without a project".
func (s *HubService) GlobalInstalls() []*GlobalArtifact { return s.globallyInstalled() }

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

	// An install with no project directory is a GLOBAL install: the artifact lands in
	// the shared, version-keyed stores it would land in anyway, and its membership is
	// recorded in the global lock instead of in a project's lockfile. Nothing else
	// about the install changes — the same version resolution, the same store work,
	// the same recursive dependencies.
	//
	// paths.GetPathsForProject must NOT be consulted in that case. With both arguments
	// empty it falls through to paths.GetPaths, which walks UP from this process's
	// working directory: a server sitting inside some checkout would bind the install
	// to that project, silently and successfully.
	globalInstall := projectDir == ""

	var pp *paths.ProjectPaths
	if !globalInstall {
		pp = paths.GetPathsForProject(ide, projectDir)
	}

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
	// measured and recorded in Graphit Task tsk-2b2208eee9b1, and stated again in
	// internal/ast/icebug_transfer.go where a caller will meet them.
	mountArtifact := s.registry.MountsArtifact(artType)
	if artType == TypeKnowledge && s.registry.IsReady() && !lancestore.Available() {
		return nil, fmt.Errorf("installing knowledge artifact %q: %w", realID, lancestore.ErrNotBuilt)
	}

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

			// Materialising into an IDE directory needs a project to put it in. A
			// global install stops at the clone, which is not a degraded outcome:
			// the clone in the shared cache IS the artifact, and it is what
			// hub content serves to a caller that has no checkout to read files from.
			if artType != TypeRule && ide != "" && !globalInstall {
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

	memberIDs := make([]string, 0, len(entry.Dependencies))
	for _, dep := range entry.Dependencies {
		memberIDs = append(memberIDs, dep.ID)
	}

	installedBy := []string{}
	if parentID != "" {
		installedBy = []string{parentID}
	}

	versionHash := ""
	if entry.Hashes != nil {
		versionHash = entry.Hashes[resolvedVersion]
	}

	owner := store.GlobalOwnerKey
	ownerDir := ""
	alreadyClaimed := map[string]bool{}

	if !globalInstall {
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

		owner = lf.Project.ID
		ownerDir = filepath.Dir(pp.LockFilePath)
		for _, typeMap := range lf.Artifacts {
			for depID := range typeMap {
				alreadyClaimed[depID] = true
			}
		}
	} else {
		for _, rec := range s.globallyInstalled() {
			alreadyClaimed[rec.ID] = true
		}
	}

	if s.lockMgr != nil {
		if _, err := s.lockMgr.RegisterInstall(InstallRecord{
			ID:          realID,
			Version:     resolvedVersion,
			Type:        artType,
			Name:        entry.Name,
			Description: entry.Description,
			Hash:        versionHash,
			CachePath:   cachePath,
			PublisherID: entry.ProjectID,
			Owner:       owner,
			OwnerDir:    ownerDir,
			LocalPath:   cloneDir,
		}); err != nil {
			s.log().Warn("register install", "id", realID, "version", resolvedVersion, "error", err)
		}
	}

	if err := s.postInstallHook(ctx, artType, realID, cloneDir, pp); err != nil {
		return nil, err
	}

	for _, dep := range entry.Dependencies {

		// The set of what is already claimed comes from the project's lockfile, or
		// from the global lock when there is no project. Same question, two records.
		if alreadyClaimed[dep.ID] {
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

	if _, err := s.lockMgr.RegisterInstall(InstallRecord{
		ID:          entryID,
		Version:     version,
		Type:        artType,
		Name:        name,
		Description: description,
		Hash:        versionHash,
		Owner:       projectID,
		OwnerDir:    projectDir,
	}); err != nil {
		s.log().Warn("register publish in global lock", "id", entryID, "version", version, "error", err)
	}
}

// UninstallGlobal drops an install that belongs to no project.
//
// It is a separate method rather than a branch inside Uninstall because Uninstall is
// built around the project lockfile: it reads the artifact's type and version from
// there, walks its members from there, decrements InstalledBy there, and removes the
// materialised copy from an IDE directory. A global install has none of that — the
// global lock IS the record — so threading a flag through would leave most of the
// function unreachable and the rest reading from a lockfile that does not exist.
//
// The shared store is only collected when the artifact comes out orphaned, which is the
// same condition a project-scoped uninstall uses: another project may still be pinned to
// this version.
func (s *HubService) UninstallGlobal(ctx context.Context, entryID string, entryType ArtifactType) error {
	if s.lockMgr == nil {
		return fmt.Errorf("the global lock is unavailable, so a global install cannot be dropped")
	}

	realID, reqVersion := entryID, ""
	if parts := strings.SplitN(entryID, "@", 2); len(parts) == 2 {
		realID, reqVersion = parts[0], parts[1]
	}

	art, err := s.findGlobalInstall(realID, entryType, reqVersion)
	if err != nil {
		return err
	}

	orphaned, err := s.lockMgr.RegisterUninstall(art.ID, art.Version, art.Type, store.GlobalOwnerKey)
	if err != nil {
		return fmt.Errorf("dropping the global install of %s@%s: %w", art.ID, art.Version, err)
	}

	if orphaned {
		if art.Type == TypeAST {
			s.cleanupSharedASTStore(&LockfileArtifactMeta{
				Version:   art.Version,
				ProjectID: art.ProjectID,
			}, art.ID)
		}
		if _, gcErr := s.lockMgr.GCOrphans(); gcErr != nil {
			s.log().Warn("GC orphans", "after", art.ID, "error", gcErr)
		}
	}

	s.tracker.TrackEvent("artifact.uninstall", "",
		map[string]string{"type": string(art.Type), "version": art.Version},
		map[string]string{"ide": ""})
	return nil
}

// findGlobalInstall resolves one globally installed artifact from a reference that may
// or may not carry a version and may or may not carry a type.
//
// An ambiguous reference is REFUSED rather than resolved by picking one. Two versions of
// the same artifact are two different stores, and silently dropping the wrong one is not
// a mistake the caller can see afterwards.
func (s *HubService) findGlobalInstall(id string, artType ArtifactType, version string) (*GlobalArtifact, error) {
	var matches []*GlobalArtifact
	for _, art := range s.globallyInstalled() {
		if art.ID != id {
			continue
		}
		if artType != "" && art.Type != artType {
			continue
		}
		if version != "" && art.Version != version {
			continue
		}
		matches = append(matches, art)
	}

	switch len(matches) {
	case 0:
		return nil, fmt.Errorf("%q is not installed globally — install it first with a hub install that omits project_dir", id)
	case 1:
		return matches[0], nil
	}

	sort.Slice(matches, func(i, j int) bool {
		if matches[i].Type != matches[j].Type {
			return matches[i].Type < matches[j].Type
		}
		return matches[i].Version < matches[j].Version
	})
	refs := make([]string, 0, len(matches))
	for _, m := range matches {
		refs = append(refs, fmt.Sprintf("%s %s@%s", m.Type, m.ID, m.Version))
	}
	return nil, fmt.Errorf("%q is installed globally more than once — name which one with an @version "+
		"suffix and a type: %s", id, strings.Join(refs, ", "))
}

// GlobalInstall resolves one globally installed artifact for callers outside this
// package. See findGlobalInstall for how an ambiguous reference is treated.
func (s *HubService) GlobalInstall(id string, artType ArtifactType, version string) (*GlobalArtifact, error) {
	return s.findGlobalInstall(id, artType, version)
}

func (s *HubService) Uninstall(
	ctx context.Context,
	entryID string,
	entryType ArtifactType,
	forceRoot bool,
	ide, projectDir string,
) error {
	if projectDir == "" {
		return s.UninstallGlobal(ctx, entryID, entryType)
	}
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

	if artType != TypeRule && !ideIndependentTypes[artType] && ide != "" {
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

	case TypeRule:
		// Rules are resident instructions delivered by the lifecycle hook. A local
		// link therefore points directly at an artifact directory containing
		// RULE.md; it never creates an IDE-specific file or symlink.
		if findCanonicalFile(string(TypeRule), absSource) == "" {
			return nil, fmt.Errorf("source rule not found at %s — expected RULE.md", absSource)
		}
		result.Links = append(result.Links, artifactName+" → dynamic hook context from "+absSource)

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

	case TypeRule:
		// Dynamic rule links have no project-local IDE artifact to remove.

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

// ResolveKnowledgeMount resolves a versioned knowledge artifact to the immutable Lance index it
// publishes. It never downloads, imports, or registers a transient local copy.
func (s *HubService) ResolveKnowledgeMount(ctx context.Context, artifactID string) (MountedWiki, error) {

	reqVersion := ""
	realID := artifactID
	if parts := strings.SplitN(artifactID, "@", 2); len(parts) == 2 {
		realID, reqVersion = parts[0], parts[1]
	}

	entry := s.registry.GetEntry(realID, TypeKnowledge)
	if entry == nil {
		return MountedWiki{}, fmt.Errorf("knowledge artifact %q not found in hub registry", realID)
	}

	resolvedVersion, err := resolveEntryVersion(entry, reqVersion)
	if err != nil {
		return MountedWiki{}, err
	}

	if !s.registry.IsReady() {
		return MountedWiki{}, fmt.Errorf("hub registry not available — cannot mount knowledge artifact %q", realID)
	}
	if !s.registry.MountsKnowledge() {
		return MountedWiki{}, fmt.Errorf("mounting knowledge artifact %s@%s: %w", realID, resolvedVersion, lancestore.ErrNotBuilt)
	}
	mount, ok := s.registry.Store().MountedWikiAt(realID, resolvedVersion, entry.ProjectID)
	if !ok {
		return MountedWiki{}, fmt.Errorf("cannot resolve mount for knowledge artifact %s@%s", realID, resolvedVersion)
	}
	return mount, nil
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
