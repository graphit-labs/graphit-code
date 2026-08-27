// Package store resolves where every compiled artifact of this framework lives.
//
// There is exactly one location per artifact, and it is in the global brand
// directory, keyed by an identifier:
//
//	<global>/ast/project/<project-id>/ladybugdb        the project's code graph
//	<global>/ast/context/<name>/ladybugdb               a locally imported graph
//	<global>/ast/hub/<context-id>/<version>/ladybugdb   a Hub graph, shared per version
//	<global>/wiki/knowledge/project/<project-id>/       the project's documentation wiki
//	<global>/wiki/knowledge/context/<name>/             an imported documentation wiki
//	<global>/wiki/memory/<scope>/<scope-id>/            a memory wiki
//	<global>/memory-raw/memory-<scope>-<scope-id>/      the raw memory markdown
//
// Nothing is copied into a project. A project used to carry a replica of each of
// these, which cost disk, cost a compile per copy, and needed sync logic whose only
// job was to keep the copies from disagreeing — and when it lost, a project answered
// with yesterday's data while looking perfectly healthy. The single copy removes the
// class of bug rather than the instances.
//
// What a project does keep is the record of WHICH artifacts belong to it. That is
// membership, not data: it is a few hundred bytes, it is genuinely per-project, and
// it cannot be derived from a global directory listing without telling project A
// about project B's imports. It lives in the context registry, see registry.go.
package store

import (
	"crypto/sha256"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"regexp"
	"runtime"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

const (
	// KindAST and KindKnowledge name the two artifact families a project can
	// import. Memory contexts are not registered here: a memory context is a
	// prefix in the memory bucket, and the set of local directories is its own record.
	KindAST       = "ast"
	KindKnowledge = "knowledge"

	// DBFileName is the graph store inside an AST store directory.
	DBFileName = "ladybugdb"

	projectNamespace = "project"
	contextNamespace = "context"
	hubNamespace     = "hub"
)

// Root is the global brand directory every store hangs off.
//
// It is empty only when the user has no resolvable home directory. Callers that
// need a path regardless use the *For helpers, which degrade to the project's own
// dot directory rather than returning nothing.
func Root() string { return brand.GlobalDir() }

// ProjectID is the project's identity as recorded in its lockfile, or "" when the
// project has not been initialised.
//
// The lockfile is read directly rather than through internal/hub because the
// packages that need this — ast, knowledge, memory — are imported BY hub.
func ProjectID(projectDir string) string {
	if projectDir == "" {
		return ""
	}
	data, err := os.ReadFile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil {
		return ""
	}
	var lock struct {
		Project struct {
			ID string `json:"id"`
		} `json:"project"`
	}
	if json.Unmarshal(data, &lock) != nil {
		return ""
	}
	return lock.Project.ID
}

// IsEphemeralProject reports whether a directory is a throwaway workspace rather
// than a project someone works in.
//
// A live search session builds a real-looking project: it has a lockfile with an
// identity, because the agent CLI discovers everything from its working directory
// and would otherwise find an empty folder. That lockfile is also what every store
// resolver keys on, so without a way to tell the two apart a session that exists for
// one search acquires a code graph, a documentation wiki and a memory scope of its
// own — none of which anything reclaims when the session is deleted.
//
// So the fact is recorded rather than deduced. "Has no source of its own" would
// catch a legitimately empty project too, and being empty is a state a real project
// passes through on its first day.
func IsEphemeralProject(projectDir string) bool {
	if projectDir == "" {
		return false
	}
	data, err := os.ReadFile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil {
		return false
	}
	var lock struct {
		Project struct {
			Ephemeral bool `json:"ephemeral"`
		} `json:"project"`
	}
	if json.Unmarshal(data, &lock) != nil {
		return false
	}
	return lock.Project.Ephemeral
}

// ProjectStoreID is the directory name a project's stores are filed under.
//
// It is the lockfile ID wherever there is one, so that two checkouts of the same
// project on one machine share a store and a project keeps its store across a move.
// An uninitialised directory still has to be indexable — `ast index` has never
// required `init` — so it falls back to a hash of the absolute path, which is
// stable, collision-free in practice, and unmistakably not a ULID.
//
// The consequence is worth stating: running `init` in a directory that was already
// indexed re-keys its store, and the first query afterwards reindexes. That is a
// one-time cost at the moment a project gains an identity, and the alternative —
// refusing to index until init — is worse.
func ProjectStoreID(projectDir string) string {
	if id := ProjectID(projectDir); id != "" {
		return storeIDSegment(id)
	}
	abs, err := filepath.Abs(projectDir)
	if err != nil {
		abs = filepath.Clean(projectDir)
	}
	return pathStoreID(abs, caseInsensitivePaths)
}

// pathStoreID derives a store id from a path, folding case when the filesystem does.
//
// Windows always, and macOS by default, treat two paths differing only in letter case
// as the SAME directory. Hashing them unfolded would give one project two store ids
// depending on how its path was typed — and therefore two graphs and two wikis, each
// looking perfectly healthy while holding half the answers.
//
// The fold is a parameter rather than a package variable read inline so that both
// behaviours are testable on any host; the platform decides the value, once.
func pathStoreID(abs string, fold bool) string {
	key := filepath.ToSlash(abs)
	if fold {
		key = strings.ToLower(key)
	}
	sum := sha256.Sum256([]byte(key))
	return "path-" + fmt.Sprintf("%x", sum)[:16]
}

// caseInsensitivePaths says whether two paths differing only in letter case name the
// same directory here.
var caseInsensitivePaths = runtime.GOOS == "windows" || runtime.GOOS == "darwin"

// storeIDSegment makes an identity usable as a directory name without changing what
// it is.
//
// It only defuses Windows device names; case and characters are left alone, because
// an id is an identity and two resolvers keyed by the same id must agree on the path.
// A project id is a ULID and will never be a device name, but ProjectStoreID also
// answers for an id that came from a global lock written by an older version, and
// HubContextID falls back to a Hub artifact ID its author chose freely.
func storeIDSegment(id string) string { return DefuseReservedName(id) }

var nonNameChars = regexp.MustCompile(`[^a-z0-9_-]`)

// SanitizeName reduces a caller-supplied context name to something usable as a
// directory name on every platform this ships to.
func SanitizeName(name string) string {
	name = strings.ToLower(strings.TrimSpace(name))
	name = strings.ReplaceAll(name, " ", "-")
	name = nonNameChars.ReplaceAllString(name, "")
	if name == "" {
		name = "unnamed"
	}
	return DefuseReservedName(name)
}

// windowsReservedNames are device names Windows refuses as a file or directory
// name, with or without an extension.
var windowsReservedNames = map[string]bool{
	"con": true, "prn": true, "aux": true, "nul": true,
	"com1": true, "com2": true, "com3": true, "com4": true, "com5": true,
	"com6": true, "com7": true, "com8": true, "com9": true,
	"lpt1": true, "lpt2": true, "lpt3": true, "lpt4": true, "lpt5": true,
	"lpt6": true, "lpt7": true, "lpt8": true, "lpt9": true,
}

// DefuseReservedName suffixes a Windows device name so it becomes an ordinary
// directory name.
//
// It runs on every platform, not only Windows, so that one artifact resolves to the
// same path everywhere: a global directory carried to another machine — or a shared
// CI image — must agree on the path, and a platform-conditional name would silently
// rebuild the store.
func DefuseReservedName(s string) string {
	base := s
	if i := strings.IndexByte(base, '.'); i >= 0 {
		base = base[:i]
	}
	if windowsReservedNames[strings.ToLower(base)] {
		return s + "_"
	}
	return s
}

// SanitizeSegment keeps a version string usable as a directory name without
// mangling the dots that make it a version. SanitizeName would strip them.
func SanitizeSegment(s string) string {
	s = strings.TrimSpace(s)
	var b strings.Builder
	for _, r := range s {
		switch {
		case r >= 'a' && r <= 'z', r >= 'A' && r <= 'Z', r >= '0' && r <= '9',
			r == '.', r == '-', r == '_', r == '+':
			b.WriteRune(r)
		default:
			b.WriteRune('-')
		}
	}
	out := strings.Trim(b.String(), ".-")
	if out == "" {
		return "unversioned"
	}
	return DefuseReservedName(out)
}

// globalOr returns <global>/<parts...>, degrading to the project's own runtime
// directory when there is no home directory to put a shared store in. Nothing is
// shared in that degenerate case, which is the honest outcome — but a path is still
// returned, so indexing works on a machine with no HOME.
//
// The fallback goes under brand.RuntimeSubdir rather than straight into the brand
// directory because a store is machine state, and that is the one directory the
// generated .gitignore covers.
func globalOr(projectDir string, parts ...string) string {
	if d := Root(); d != "" {
		return filepath.Join(append([]string{d}, parts...)...)
	}
	return brand.ProjectRuntimePath(projectDir, parts...)
}

// ASTProjectDir is the store directory for a project's own code graph. The graph,
// its search index, the shard manifest and the shard tree all live inside it.
func ASTProjectDir(projectDir string) string {
	return globalOr(projectDir, "ast", projectNamespace, ProjectStoreID(projectDir))
}

// ASTProjectDBPath is the graph file inside ASTProjectDir.
func ASTProjectDBPath(projectDir string) string {
	return filepath.Join(ASTProjectDir(projectDir), DBFileName)
}

// ASTProjectDirByID is ASTProjectDir for a project whose id is already known — from
// the global lock, say — so its lockfile does not have to be read again.
//
// It goes through storeIDSegment for the same reason ProjectStoreID does: the two must
// resolve to the same directory for the same id, or a project's own graph and the same
// graph seen from the ecosystem are two different stores.
func ASTProjectDirByID(projectID string) string {
	return globalOr("", "ast", projectNamespace, storeIDSegment(projectID))
}

// ASTContextDir is the store directory for a locally imported code graph.
//
// It is keyed by name alone, with no project in the path: the same checkout imported
// by two projects is one graph, and indexing it twice would buy nothing. Which
// projects may query it is recorded in each project's registry.
func ASTContextDir(name string) string {
	return globalOr("", "ast", contextNamespace, SanitizeName(name))
}

// ASTContextDBPath is the graph file inside ASTContextDir.
func ASTContextDBPath(name string) string {
	return filepath.Join(ASTContextDir(name), DBFileName)
}

// ASTHubRoot is the parent of every version-scoped Hub AST store.
func ASTHubRoot() string { return globalOr("", "ast", hubNamespace) }

// ASTHubDir is the shared directory holding one version of a Hub AST context.
//
// A published AST artifact is immutable at a given version — the graph was built
// upstream and nothing in a consuming project can change it — so one store per
// version serves every project pinned to it, and two projects on different versions
// still get their own.
func ASTHubDir(contextID, version string) string {
	return filepath.Join(ASTHubRoot(), SanitizeName(contextID), SanitizeSegment(version))
}

// ASTHubDBPath is the graph file inside ASTHubDir.
func ASTHubDBPath(contextID, version string) string {
	return filepath.Join(ASTHubDir(contextID, version), DBFileName)
}

// KnowledgeProjectDir is the compiled documentation wiki of one project.
func KnowledgeProjectDir(projectDir string) string {
	return globalOr(projectDir, "wiki", "knowledge", projectNamespace, ProjectStoreID(projectDir))
}

// KnowledgeProjectDirByID is KnowledgeProjectDir for a project whose id is already
// known — from the global lock, say — so its lockfile does not have to be read again.
// It is also the only form that works for a project whose directory this process
// cannot stat.
func KnowledgeProjectDirByID(projectID string) string {
	return globalOr("", "wiki", "knowledge", projectNamespace, storeIDSegment(projectID))
}

// KnowledgeContextDir is the compiled documentation wiki of a LOCALLY imported
// context — one indexed from a directory on this machine, which has no version.
func KnowledgeContextDir(name string) string {
	return globalOr("", "wiki", "knowledge", contextNamespace, SanitizeName(name))
}

// KnowledgeHubRoot is the parent of every Hub documentation wiki on this machine.
func KnowledgeHubRoot() string { return globalOr("", "wiki", "knowledge", hubNamespace) }

// KnowledgeHubDir is the compiled documentation wiki of a Hub artifact, shared by every
// project pinned to that version.
//
// The version is part of the path, mirroring ASTHubDir. It did not use to be: a Hub
// knowledge artifact was installed into KnowledgeContextDir, so two projects pinned to
// different versions shared one directory and the last install silently won, while both
// lockfiles recorded versions that nothing enforced.
func KnowledgeHubDir(contextID, version string) string {
	return filepath.Join(KnowledgeHubRoot(), SanitizeName(contextID), SanitizeSegment(version))
}

// MemoryWikiDir is the compiled wiki of one memory scope. Scopes are "project",
// "user", or the name of an imported context.
func MemoryWikiDir(scope, scopeID string) string {
	return globalOr("", "wiki", "memory", SanitizeSegment(scope), SanitizeSegment(scopeID))
}

// MemoryRawRoot is the parent of every scope's raw memory directory.
func MemoryRawRoot() string { return globalOr("", "memory-raw") }

// MemoryRawDir is the directory holding a scope's raw memory markdown — the
// source of truth the wiki is compiled from, and where a memory arriving from the
// remote lands.
func MemoryRawDir(scope, scopeID string) string {
	branch := fmt.Sprintf("memory/%s/%s", scope, scopeID)
	safe := strings.NewReplacer("/", "-", " ", "_").Replace(branch)
	return filepath.Join(MemoryRawRoot(), safe)
}
