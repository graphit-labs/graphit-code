// Package projectversion resolves which version of a project a store belongs to.
//
// A project's identity is its lockfile ULID, and every branch and worktree of a
// repository shares that one lockfile. The code each of them holds is different,
// though, so every project store — code graph, documentation wiki, memory, tasks,
// dream runs — is kept per version: the checked-out Git branch, or "main" for a
// directory that is not under Git and therefore only ever has one.
//
// The package reads Git's own files instead of spawning git, because it is asked on
// every store resolution, including from the lightweight hook binaries. It has no
// cgo dependency for the same reason.
package projectversion

import (
	"context"
	"encoding/hex"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"sync"
	"time"
	"unicode"

	"github.com/graphit-labs/graphit-code/internal/gitenv"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

// DefaultName is the version of a project that is not under Git, and the fallback
// default version of a repository whose default branch cannot be determined.
const DefaultName = "main"

// Info is the resolved version of one project directory.
type Info struct {
	// Version is the effective version every store of this call uses.
	Version string
	// Default is the project's default version: the lineage new versions fork from
	// and the version legacy, unversioned stores belong to.
	Default string
	// Git reports whether the directory is inside a Git working tree.
	Git bool
	// Detached reports a detached HEAD, resolved through the base-branch variable or
	// the default version.
	Detached bool
	// Explicit reports that Version came from a caller rather than from the checkout.
	Explicit bool
}

// IsDefault reports whether this is the project's default version.
func (i Info) IsDefault() bool { return i.Version == i.Default }

// LanceBranch is the native Lance branch holding this version, "" for the main lineage.
func (i Info) LanceBranch() string { return LanceBranchName(i.Version, i.Default) }

// EnvVersion names the variable that pins the version of the project the process was
// started in. Child processes inherit it from a parent given an explicit version.
func EnvVersion() string { return brand.EnvVar("PROJECT_VERSION") }

// EnvBaseBranch names the variable a detached checkout (CI, for example) uses to say
// which branch it represents.
func EnvBaseBranch() string { return brand.EnvVar("GIT_BASE_BRANCH") }

// Validate normalizes a caller-supplied version. Empty means "resolve from the checkout".
func Validate(version string) (string, error) {
	version = strings.TrimSpace(version)
	for _, r := range version {
		if unicode.IsControl(r) {
			return "", fmt.Errorf("invalid project version %q: control characters are not allowed", version)
		}
	}
	return version, nil
}

// Resolve answers the version of projectDir. A non-empty explicit version wins; then
// the process-wide pin from EnvVersion, which applies only to the project the process
// was started in; then the checkout itself.
func Resolve(projectDir, explicit string) (Info, error) {
	explicit, err := Validate(explicit)
	if err != nil {
		return Info{}, err
	}
	info := inspect(projectDir)
	if explicit != "" {
		info.Version, info.Explicit = explicit, true
		return info, nil
	}
	if pinned := envPin(projectDir); pinned != "" {
		info.Version, info.Explicit = pinned, true
	}
	return info, nil
}

// Current is the version a store resolver uses for projectDir: a version pinned for
// the running call by Acquire, otherwise Resolve without an explicit version.
func Current(projectDir string) Info {
	info, err := Resolve(projectDir, override(projectDir))
	if err != nil {
		return inspect(projectDir)
	}
	return info
}

// LanceBranchName maps a version to the native Lance branch that holds it. The
// default version is the dataset's main lineage and has no branch. Any other version
// keeps its own name when Lance accepts it as a branch name, and is otherwise encoded
// as "h-" plus its hexadecimal UTF-8 bytes — which also covers a non-default version
// literally called "main", a name Lance reserves.
func LanceBranchName(version, defaultVersion string) string {
	if version == "" || version == defaultVersion {
		return ""
	}
	if validLanceBranch(version) {
		return version
	}
	return "h-" + hex.EncodeToString([]byte(version))
}

func validLanceBranch(name string) bool {
	if name == "main" || strings.HasPrefix(name, "h-") || strings.Contains(name, "..") ||
		strings.HasSuffix(name, ".lock") {
		return false
	}
	for _, segment := range strings.Split(name, "/") {
		if segment == "" {
			return false
		}
		for _, r := range segment {
			if !isLanceBranchRune(r) {
				return false
			}
		}
	}
	return true
}

// isLanceBranchRune reports the characters Lance accepts in a branch name segment.
func isLanceBranchRune(r rune) bool {
	return r >= 'a' && r <= 'z' || r >= 'A' && r <= 'Z' || r >= '0' && r <= '9' ||
		r == '.' || r == '-' || r == '_'
}

// Scoped overrides let a long-running server answer one call for an explicit version
// while other calls for the same directory keep resolving from the checkout. A call
// with an explicit version holds the directory exclusively; calls without one share it.
var (
	scopeMu   sync.Mutex
	scopes    = map[string]*sync.RWMutex{}
	overrides sync.Map // clean dir -> version
)

// Acquire scopes the running call to projectDir's version until release is called.
// With an explicit version every Current(projectDir) inside the scope answers it.
func Acquire(projectDir, explicit string) (release func(), err error) {
	return AcquireContext(context.Background(), projectDir, explicit)
}

// AcquireContext is Acquire with cancelable waiting, for server and daemon jobs.
// A canceled waiter never publishes an override or leaves a lock-owning goroutine.
func AcquireContext(ctx context.Context, projectDir, explicit string) (release func(), err error) {
	explicit, err = Validate(explicit)
	if err != nil {
		return nil, err
	}
	return acquireKey(ctx, scopeKey(projectDir), explicit)
}

// acquireKey holds the scope named key: shared without an explicit version, exclusive
// with one, which it publishes as that scope's override until release.
func acquireKey(ctx context.Context, key, explicit string) (release func(), err error) {
	if key == "" {
		return func() {}, nil
	}
	scopeMu.Lock()
	mu := scopes[key]
	if mu == nil {
		mu = &sync.RWMutex{}
		scopes[key] = mu
	}
	scopeMu.Unlock()
	ticker := time.NewTicker(10 * time.Millisecond)
	defer ticker.Stop()
	for {
		if err := ctx.Err(); err != nil {
			return nil, err
		}
		if explicit == "" {
			if mu.TryRLock() {
				return mu.RUnlock, nil
			}
		} else if mu.TryLock() {
			break
		}
		select {
		case <-ctx.Done():
			return nil, ctx.Err()
		case <-ticker.C:
		}
	}
	overrides.Store(key, explicit)
	return func() {
		overrides.Delete(key)
		mu.Unlock()
	}, nil
}

func override(projectDir string) string {
	if v, ok := overrides.Load(scopeKey(projectDir)); ok {
		return v.(string)
	}
	return ""
}

func scopeKey(projectDir string) string {
	if projectDir == "" {
		return ""
	}
	abs, err := filepath.Abs(projectDir)
	if err != nil {
		return filepath.Clean(projectDir)
	}
	return abs
}

var (
	envRootOnce sync.Once
	envRoot     string
)

// envPin is the version pinned by EnvVersion, applied only to the project the process
// started in. A long-running process reaching a sibling project must not carry its
// own project's pin over to it.
func envPin(projectDir string) string {
	pinned, _ := Validate(os.Getenv(EnvVersion()))
	if pinned == "" {
		return ""
	}
	envRootOnce.Do(func() {
		if wd, err := os.Getwd(); err == nil {
			envRoot = projectRoot(wd)
		}
	})
	if envRoot == "" || projectRoot(projectDir) != envRoot {
		return ""
	}
	return pinned
}

// projectRoot is the nearest ancestor holding the lockfile or a Git entry, which is
// how one checkout is told apart from another.
func projectRoot(dir string) string {
	abs, err := filepath.Abs(dir)
	if err != nil {
		return ""
	}
	for d := abs; ; d = filepath.Dir(d) {
		if exists(filepath.Join(d, brand.LockFileName())) || exists(filepath.Join(d, ".git")) {
			return d
		}
		if parent := filepath.Dir(d); parent == d {
			return abs
		}
	}
}

// inspect resolves the checkout's own version, ignoring every override.
func inspect(projectDir string) Info {
	repo, ok := findRepository(projectDir)
	if !ok {
		return Info{Version: DefaultName, Default: DefaultName}
	}
	info := Info{Git: true}
	branch, detached := repo.head()
	info.Default = repo.defaultBranch(branch)
	info.Detached = detached
	switch {
	case !detached && branch != "":
		info.Version = branch
	default:
		if base := strings.TrimPrefix(strings.TrimSpace(os.Getenv(EnvBaseBranch())), "refs/heads/"); base != "" {
			info.Version = base
		} else {
			info.Version = info.Default
		}
	}
	return info
}

type repository struct {
	workTree  string
	gitDir    string
	commonDir string
}

func findRepository(projectDir string) (repository, bool) {
	if projectDir == "" {
		return repository{}, false
	}
	abs, err := filepath.Abs(projectDir)
	if err != nil {
		return repository{}, false
	}
	for d := abs; ; d = filepath.Dir(d) {
		dotGit := filepath.Join(d, ".git")
		if st, err := os.Stat(dotGit); err == nil {
			if st.IsDir() {
				return repository{workTree: d, gitDir: dotGit, commonDir: commonDirOf(dotGit)}, true
			}
			if gitDir := readGitFile(dotGit); gitDir != "" {
				return repository{workTree: d, gitDir: gitDir, commonDir: commonDirOf(gitDir)}, true
			}
		}
		if parent := filepath.Dir(d); parent == d {
			return repository{}, false
		}
	}
}

// readGitFile follows the "gitdir:" pointer a linked worktree or submodule has in
// place of a .git directory.
func readGitFile(path string) string {
	data, err := os.ReadFile(path)
	if err != nil {
		return ""
	}
	line := strings.TrimSpace(string(data))
	target, ok := strings.CutPrefix(line, "gitdir:")
	if !ok {
		return ""
	}
	target = strings.TrimSpace(target)
	if !filepath.IsAbs(target) {
		target = filepath.Join(filepath.Dir(path), target)
	}
	return filepath.Clean(target)
}

// commonDirOf is where refs shared by every worktree live.
func commonDirOf(gitDir string) string {
	data, err := os.ReadFile(filepath.Join(gitDir, "commondir"))
	if err != nil {
		return gitDir
	}
	common := strings.TrimSpace(string(data))
	if common == "" {
		return gitDir
	}
	if !filepath.IsAbs(common) {
		common = filepath.Join(gitDir, common)
	}
	return filepath.Clean(common)
}

// usesReftable reports the reftable ref backend, whose HEAD file is a placeholder;
// only git itself can read its refs.
func (r repository) usesReftable() bool { return exists(filepath.Join(r.commonDir, "reftable")) }

func (r repository) head() (branch string, detached bool) {
	if r.usesReftable() {
		out, err := r.git("symbolic-ref", "--quiet", "HEAD")
		if err != nil {
			return "", true
		}
		branch, ok := strings.CutPrefix(out, "refs/heads/")
		return branch, !ok
	}
	data, err := os.ReadFile(filepath.Join(r.gitDir, "HEAD"))
	if err != nil {
		return "", true
	}
	target, ok := strings.CutPrefix(strings.TrimSpace(string(data)), "ref:")
	if !ok {
		return "", true
	}
	branch, ok = strings.CutPrefix(strings.TrimSpace(target), "refs/heads/")
	if !ok || branch == "" {
		return "", true
	}
	return branch, false
}

// defaultBranch is origin's HEAD, then an existing main, then an existing master,
// then the checked-out branch of a repository that has no other (a fresh one), then
// DefaultName.
func (r repository) defaultBranch(current string) string {
	if originHead := r.symbolicRef("refs/remotes/origin/HEAD"); originHead != "" {
		if branch, ok := strings.CutPrefix(originHead, "refs/remotes/origin/"); ok && branch != "" {
			return branch
		}
	}
	for _, candidate := range []string{"main", "master"} {
		if r.refExists("refs/heads/" + candidate) {
			return candidate
		}
	}
	if current != "" && !r.anyLocalBranch() {
		return current
	}
	return DefaultName
}

func (r repository) symbolicRef(name string) string {
	if r.usesReftable() {
		out, err := r.git("symbolic-ref", "--quiet", name)
		if err != nil {
			return ""
		}
		return out
	}
	data, err := os.ReadFile(filepath.Join(r.commonDir, filepath.FromSlash(name)))
	if err != nil {
		return ""
	}
	target, ok := strings.CutPrefix(strings.TrimSpace(string(data)), "ref:")
	if !ok {
		return ""
	}
	return strings.TrimSpace(target)
}

func (r repository) refExists(name string) bool {
	if r.usesReftable() {
		_, err := r.git("show-ref", "--verify", "--quiet", name)
		return err == nil
	}
	if exists(filepath.Join(r.commonDir, filepath.FromSlash(name))) {
		return true
	}
	data, err := os.ReadFile(filepath.Join(r.commonDir, "packed-refs"))
	if err != nil {
		return false
	}
	for _, line := range strings.Split(string(data), "\n") {
		fields := strings.Fields(line)
		if len(fields) == 2 && fields[1] == name {
			return true
		}
	}
	return false
}

func (r repository) anyLocalBranch() bool {
	if r.usesReftable() {
		out, _ := r.git("for-each-ref", "--count=1", "--format=%(refname)", "refs/heads/")
		return out != ""
	}
	found := false
	_ = filepath.WalkDir(filepath.Join(r.commonDir, "refs", "heads"), func(_ string, d os.DirEntry, err error) error {
		if err == nil && !d.IsDir() {
			found = true
			return filepath.SkipAll
		}
		return nil
	})
	if found {
		return true
	}
	data, err := os.ReadFile(filepath.Join(r.commonDir, "packed-refs"))
	return err == nil && strings.Contains(string(data), " refs/heads/")
}

func (r repository) git(args ...string) (string, error) {
	cmd := exec.Command("git", append([]string{"-C", r.workTree}, args...)...)
	// Inside a Git hook the inherited invocation scope would point this command at the
	// hook's repository instead of the one being resolved.
	cmd.Env = gitenv.WithoutInvocationScope(os.Environ())
	out, err := cmd.Output()
	return strings.TrimSpace(string(out)), err
}

func exists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}
