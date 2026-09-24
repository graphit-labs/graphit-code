package ast

import (
	"context"
	"crypto/sha256"
	"encoding/csv"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path"
	"path/filepath"
	"runtime"
	"sort"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/store"
)

func scipImage(projectDir, family string) string {
	projectCfg := config.LoadProjectConfig(projectDir)
	tag := config.ResolveAstSCIPVersion(nil, projectCfg)
	return "ghcr.io/graphit-labs/graphit-scip-" + family + ":" + tag
}

func scipCacheDir(root, graphCacheDir, family string) string {
	if graphCacheDir == "" {
		return ""
	}
	return filepath.Join(graphCacheDir, "scip", scipProjectKey(root), family)
}

func scipOutputDir(root, graphCacheDir, family string) string {
	if graphCacheDir == "" {
		return ""
	}
	return filepath.Join(graphCacheDir, "scip-output", scipProjectKey(root), family)
}

func scipProjectKey(root string) string {
	sum := sha256.Sum256([]byte(filepath.Clean(root)))
	return hex.EncodeToString(sum[:16])
}

func scipContainerName(root, family string) string {
	return "graphit-scip-" + family + "-" + scipProjectKey(root)
}

func scipBindMount(source, target string, readonly bool) string {
	fields := []string{"type=bind", "src=" + source, "dst=" + target}
	if readonly {
		fields = append(fields, "readonly")
	}
	var out strings.Builder
	writer := csv.NewWriter(&out)
	_ = writer.Write(fields)
	writer.Flush()
	return strings.TrimSuffix(out.String(), "\n")
}

// Only the inferred TypeScript project uses Graphit's exact root-file list.
// Project-owned tsconfig files keep their own compilation semantics. Imported
// dependencies can still be read by TypeScript; the SCIP import filter remains
// the final authority for which documents enter Graphit.
func writeSCIPTypeScriptConfig(root, cacheDir string, allowed map[string]bool) error {
	configPath := filepath.Join(cacheDir, "graphit-tsconfig.json")
	if err := os.Remove(configPath); err != nil && !os.IsNotExist(err) {
		return err
	}
	if len(allowed) == 0 {
		return nil
	}
	if _, err := os.Stat(filepath.Join(root, "tsconfig.json")); err == nil {
		return nil
	} else if !os.IsNotExist(err) {
		return err
	}
	files := make([]string, 0, len(allowed))
	for rel := range allowed {
		if rel == "" || path.IsAbs(rel) || rel == ".." || strings.HasPrefix(rel, "../") || path.Clean(rel) != rel || strings.Contains(rel, "\\") {
			return fmt.Errorf("invalid SCIP TypeScript source path %q", rel)
		}
		files = append(files, path.Join("/workspace", rel))
	}
	sort.Strings(files)
	data, err := json.Marshal(struct {
		CompilerOptions map[string]bool `json:"compilerOptions"`
		Files           []string        `json:"files"`
	}{CompilerOptions: map[string]bool{"allowJs": true}, Files: files})
	if err != nil {
		return err
	}
	return os.WriteFile(configPath, append(data, '\n'), 0o644)
}

func writeSCIPClangAllowlist(cacheDir string, allowed map[string]bool) error {
	manifest := filepath.Join(cacheDir, "graphit-allowed-files.json")
	if err := os.Remove(manifest); err != nil && !os.IsNotExist(err) {
		return err
	}
	if allowed == nil {
		return nil
	}
	files := make([]string, 0, len(allowed))
	for rel := range allowed {
		if rel == "" || path.IsAbs(rel) || rel == ".." || strings.HasPrefix(rel, "../") || path.Clean(rel) != rel || strings.Contains(rel, "\\") {
			return fmt.Errorf("invalid SCIP Clang source path %q", rel)
		}
		files = append(files, rel)
	}
	sort.Strings(files)
	data, err := json.Marshal(files)
	if err != nil {
		return err
	}
	return os.WriteFile(manifest, append(data, '\n'), 0o644)
}

func isSCIPTypeScriptConfigPath(rel string) bool {
	base := strings.ToLower(filepath.Base(rel))
	return (strings.HasPrefix(base, "tsconfig") || strings.HasPrefix(base, "jsconfig")) && strings.HasSuffix(base, ".json")
}

// The selector records the roots TypeScript may index plus the user's config.
// It is small, deterministic, and lives beside the AST cache, never in source.
func scipTypeScriptSelectionSignature(root string, files []string, excludeExts map[string]bool) (string, int, error) {
	h := sha256.New()
	profile, profileErr := scipProfileFor(root, "typescript")
	extAllowed := make(map[string]bool)
	configBytes, err := os.ReadFile(filepath.Join(root, "tsconfig.json"))
	if err != nil && !os.IsNotExist(err) {
		return "", 0, err
	}
	if err == nil {
		_, _ = h.Write([]byte("tsconfig\x00"))
		_, _ = h.Write(configBytes)
	} else {
		_, _ = h.Write([]byte("inferred\x00"))
	}
	var selected []string
	for _, file := range files {
		if isSCIPTypeScriptConfigPath(file) {
			rel, relErr := filepath.Rel(root, file)
			if relErr == nil && filepath.ToSlash(rel) != "tsconfig.json" {
				configBytes, readErr := os.ReadFile(file)
				if readErr != nil {
					return "", 0, readErr
				}
				_, _ = h.Write([]byte("config:" + filepath.ToSlash(rel) + "\x00"))
				_, _ = h.Write(configBytes)
				_, _ = h.Write([]byte{0})
			}
		}
		ext := strings.ToLower(filepath.Ext(file))
		admitted, ok := extAllowed[ext]
		if !ok {
			admitted = !excludeExts[ext] && scipFamilyFor(root, ext) == "typescript" && (profileErr != nil || profile.supportsExt(ext))
			extAllowed[ext] = admitted
		}
		if !admitted {
			continue
		}
		rel, relErr := filepath.Rel(root, file)
		if relErr == nil {
			selected = append(selected, filepath.ToSlash(rel))
		}
	}
	// collectFiles uses filepath.Walk's lexical order, so the selector needs no
	// second sort. The wrapper sorts only when serializing its map as JSON.
	for _, rel := range selected {
		_, _ = h.Write([]byte(rel))
		_, _ = h.Write([]byte{0})
	}
	return hex.EncodeToString(h.Sum(nil)), len(selected), nil
}

func scipClangSelectionSignature(root string, files []string, excludeExts map[string]bool) (string, int, error) {
	h := sha256.New()
	var selectedConfig string
	for _, rel := range []string{"compile_commands.json", "build/compile_commands.json", "CMakeLists.txt"} {
		data, err := os.ReadFile(filepath.Join(root, filepath.FromSlash(rel)))
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return "", 0, err
		}
		_, _ = h.Write([]byte(rel + "\x00"))
		_, _ = h.Write(data)
		_, _ = h.Write([]byte{0})
		selectedConfig = rel
		break // Match the wrapper's compdb precedence.
	}
	if selectedConfig == "CMakeLists.txt" {
		err := filepath.WalkDir(root, func(name string, entry os.DirEntry, walkErr error) error {
			if walkErr != nil {
				return walkErr
			}
			if entry.IsDir() {
				if name != root && (entry.Name() == ".git" || entry.Name() == ".graphit" || entry.Name() == "node_modules") {
					return filepath.SkipDir
				}
				return nil
			}
			if name == filepath.Join(root, "CMakeLists.txt") || entry.Name() != "CMakeLists.txt" {
				return nil
			}
			data, err := os.ReadFile(name)
			if err != nil {
				return err
			}
			rel, err := filepath.Rel(root, name)
			if err != nil {
				return err
			}
			_, _ = h.Write([]byte(filepath.ToSlash(rel) + "\x00"))
			_, _ = h.Write(data)
			_, _ = h.Write([]byte{0})
			return nil
		})
		if err != nil {
			return "", 0, err
		}
	}
	profile, profileErr := scipProfileFor(root, "clang")
	extAllowed := make(map[string]bool)
	tuCount := 0
	for _, file := range files {
		ext := strings.ToLower(filepath.Ext(file))
		admitted, ok := extAllowed[ext]
		if !ok {
			admitted = !excludeExts[ext] && scipFamilyFor(root, ext) == "clang" && (profileErr != nil || profile.supportsExt(ext))
			extAllowed[ext] = admitted
		}
		if !admitted {
			continue
		}
		rel, err := filepath.Rel(root, file)
		if err != nil {
			continue
		}
		_, _ = h.Write([]byte(filepath.ToSlash(rel)))
		_, _ = h.Write([]byte{0})
		if ext == ".c" || ext == ".cpp" || ext == ".cc" || ext == ".cxx" {
			tuCount++
		}
	}
	return hex.EncodeToString(h.Sum(nil)), tuCount, nil
}

func scipHostIdentity(ctx context.Context) (int, int) {
	if runtime.GOOS != "linux" || os.Getuid() < 0 || os.Getgid() < 0 {
		// Docker Desktop maps Windows/macOS bind mounts to the invoking user.
		return 0, 0
	}
	infoCtx, cancel := context.WithTimeout(ctx, 2*time.Second)
	defer cancel()
	info, err := exec.CommandContext(infoCtx, "docker", "info", "--format", "{{.OperatingSystem}} {{json .SecurityOptions}}").Output()
	if err == nil && (strings.Contains(strings.ToLower(string(info)), "docker desktop") || strings.Contains(strings.ToLower(string(info)), "rootless")) {
		return 0, 0
	}
	return os.Getuid(), os.Getgid()
}

func scipPlatform(family string) string {
	// These upstream indexers currently publish only Linux amd64 binaries.
	// Docker Desktop can run them under emulation on Apple Silicon/Windows ARM.
	if family == "clang" || family == "ruby" {
		return "linux/amd64"
	}
	return ""
}

func scipPullPolicy() (string, error) {
	policy := strings.ToLower(strings.TrimSpace(os.Getenv("GRAPHIT_AST_SCIP_PULL_POLICY")))
	if policy == "" {
		return "always", nil
	}
	if policy != "always" && policy != "missing" {
		return "", fmt.Errorf("GRAPHIT_AST_SCIP_PULL_POLICY must be always or missing")
	}
	return policy, nil
}

type scipContainer struct {
	ID     string `json:"Id"`
	Config struct {
		Labels map[string]string `json:"Labels"`
	} `json:"Config"`
	State struct {
		Running  bool `json:"Running"`
		ExitCode int  `json:"ExitCode"`
	} `json:"State"`
}

func inspectSCIPContainer(ctx context.Context, name string) (*scipContainer, error) {
	output, err := exec.CommandContext(ctx, "docker", "container", "inspect", name).Output()
	if err != nil {
		return nil, err
	}
	var containers []scipContainer
	if err := json.Unmarshal(output, &containers); err != nil || len(containers) != 1 {
		return nil, fmt.Errorf("decode Docker container inspection: %v", err)
	}
	return &containers[0], nil
}

func ensureSCIPContainer(ctx context.Context, root, cacheDir, outDir, family string, uid, gid int) (string, error) {
	name := scipContainerName(root, family)
	image := scipImage(root, family)
	pullPolicy, err := scipPullPolicy()
	if err != nil {
		return "", err
	}
	nonce := fmt.Sprintf("%d-%d", os.Getpid(), time.Now().UnixNano())
	if existing, err := inspectSCIPContainer(ctx, name); err == nil {
		if existing.State.Running {
			return "", fmt.Errorf("SCIP container %s is already running", name)
		}
		if output, err := exec.CommandContext(ctx, "docker", "container", "rm", "-v", existing.ID).CombinedOutput(); err != nil {
			return "", fmt.Errorf("replace SCIP container %s: %w: %s", name, err, strings.TrimSpace(string(output)))
		}
	}
	args := []string{"container", "create", "--pull=" + pullPolicy, "--quiet", "--name", name,
		"--label", "graphit.scip.nonce=" + nonce,
		"--cap-add", "SYS_ADMIN", "--security-opt", "apparmor=unconfined",
		"--mount", scipBindMount(root, "/source", true),
		"--mount", scipBindMount(root, "/workspace", true),
		"--mount", "type=volume,dst=/overlay,volume-nocopy"}
	args = append(args,
		"--mount", scipBindMount(outDir, "/output", false),
		"--mount", scipBindMount(cacheDir, "/cache", false),
		"--env", "GRAPHIT_SCIP_OVERLAY=1",
		"--env", "GRAPHIT_SCIP_UID="+strconv.Itoa(uid),
		"--env", "GRAPHIT_SCIP_GID="+strconv.Itoa(gid),
		"--env", "GRAPHIT_SCIP_PROJECT_NAME="+filepath.Base(root),
		"--env", "GRAPHIT_SCIP_HOST_ROOT="+root)
	if platform := scipPlatform(family); platform != "" {
		args = append(args, "--platform", platform)
	}
	args = append(args, image)
	cmd := exec.CommandContext(ctx, "docker", args...)
	var stderr strings.Builder
	cmd.Stderr = &stderr
	output, err := cmd.Output()
	if err != nil {
		// The CLI can be interrupted after the daemon created the container.
		// Reconcile the known name so its anonymous overlay volume is not leaked.
		cleanupCtx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		if existing, inspectErr := inspectSCIPContainer(cleanupCtx, name); inspectErr == nil && !existing.State.Running && existing.Config.Labels["graphit.scip.nonce"] == nonce {
			if cleanupOutput, cleanupErr := exec.CommandContext(cleanupCtx, "docker", "container", "rm", "-v", existing.ID).CombinedOutput(); cleanupErr != nil {
				return "", fmt.Errorf("create SCIP container %s: %w: %s; remove partial container: %v: %s", name, err, strings.TrimSpace(stderr.String()), cleanupErr, strings.TrimSpace(string(cleanupOutput)))
			}
		}
		return "", fmt.Errorf("create SCIP container %s: %w: %s", name, err, strings.TrimSpace(stderr.String()))
	}
	id := strings.TrimSpace(string(output))
	if id == "" {
		return "", fmt.Errorf("create SCIP container %s returned no container ID", name)
	}
	return id, nil
}

type scipTail struct {
	mu   sync.Mutex
	data []byte
}

func (b *scipTail) Write(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	n := len(p)
	b.data = append(b.data, p...)
	if len(b.data) > 4096 {
		b.data = append([]byte(nil), b.data[len(b.data)-4096:]...)
	}
	return n, nil
}

// runSCIPImage is replaceable by tests. Each parse gets a fresh anonymous
// Docker volume for copy-on-write; output and indexer caches stay bind-mounted.
var runSCIPImage = func(ctx context.Context, root, graphCacheDir, family string, allowed ...map[string]bool) (data []byte, runErr error) {
	var err error
	root, err = filepath.Abs(root)
	if err != nil {
		return nil, err
	}
	if graphCacheDir == "" {
		graphCacheDir = store.ASTProjectDir(root)
		if graphCacheDir == "" {
			return nil, fmt.Errorf("SCIP requires an initialized project AST directory")
		}
	}
	graphCacheDir, err = filepath.Abs(graphCacheDir)
	if err != nil {
		return nil, err
	}
	if rel, err := filepath.Rel(root, graphCacheDir); err == nil && rel != ".." && !strings.HasPrefix(rel, ".."+string(filepath.Separator)) {
		return nil, fmt.Errorf("SCIP AST directory must be outside the source project")
	}
	cacheDir, err := filepath.Abs(scipCacheDir(root, graphCacheDir, family))
	if err != nil {
		return nil, err
	}
	outDir, err := filepath.Abs(scipOutputDir(root, graphCacheDir, family))
	if err != nil {
		return nil, err
	}
	for _, dir := range []string{cacheDir, outDir} {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			return nil, err
		}
	}
	uid, gid := scipHostIdentity(ctx)
	if family == "typescript" {
		var selected map[string]bool
		if len(allowed) > 0 {
			selected = allowed[0]
		}
		if err := writeSCIPTypeScriptConfig(root, cacheDir, selected); err != nil {
			return nil, err
		}
	} else if family == "clang" {
		var selected map[string]bool
		if len(allowed) > 0 {
			selected = allowed[0]
		}
		if err := writeSCIPClangAllowlist(cacheDir, selected); err != nil {
			return nil, err
		}
	}
	indexPath := filepath.Join(outDir, "index.scip")
	if err := os.Remove(indexPath); err != nil && !os.IsNotExist(err) {
		return nil, err
	}
	containerID, err := ensureSCIPContainer(ctx, root, cacheDir, outDir, family, uid, gid)
	if err != nil {
		return nil, err
	}
	defer func() {
		cleanupCtx, cancel := context.WithTimeout(context.Background(), 15*time.Second)
		defer cancel()
		if output, err := exec.CommandContext(cleanupCtx, "docker", "container", "rm", "-f", "-v", containerID).CombinedOutput(); err != nil {
			cleanupErr := fmt.Errorf("remove SCIP container and overlay volume %s: %w: %s", containerID, err, strings.TrimSpace(string(output)))
			if runErr == nil {
				runErr = cleanupErr
			} else {
				runErr = fmt.Errorf("%w; %v", runErr, cleanupErr)
			}
		}
	}()
	cmd := exec.CommandContext(ctx, "docker", "container", "start", "--attach", containerID)
	var logs scipTail
	cmd.Stdout, cmd.Stderr = &logs, &logs
	err = cmd.Run()
	if ctx.Err() != nil {
		return nil, ctx.Err()
	}
	state, inspectErr := inspectSCIPContainer(ctx, containerID)
	if err != nil || inspectErr != nil || state.State.ExitCode != 0 {
		return nil, fmt.Errorf("docker SCIP %s failed: start=%v inspect=%v exit=%v: %s", family, err, inspectErr,
			func() int {
				if state != nil {
					return state.State.ExitCode
				}
				return -1
			}(), strings.TrimSpace(string(logs.data)))
	}
	data, err = os.ReadFile(indexPath)
	if err != nil {
		return nil, fmt.Errorf("docker SCIP %s produced no index.scip: %w", family, err)
	}
	if len(data) == 0 {
		return nil, fmt.Errorf("docker SCIP %s produced an empty index.scip", family)
	}
	return data, nil
}

// prepareSCIPEntries runs each relevant indexer once for this pipeline, then
// returns only files admitted by the same ignore checker as ordinary discovery.
// Failure is per family: callers simply let those files use existing parsers.
type scipSelection struct {
	files      []string
	forceTS    bool
	forceClang bool
}

func prepareSCIPEntries(ctx context.Context, root, graphCacheDir string, changed []string, excludeExts map[string]bool, selection ...scipSelection) (map[string]*parseCacheEntry, []error) {
	if len(changed) == 0 && (len(selection) == 0 || !selection[0].forceTS && !selection[0].forceClang) {
		return nil, nil
	}
	families := make(map[string]bool)
	for _, file := range changed {
		if excludeExts[strings.ToLower(filepath.Ext(file))] {
			continue
		}
		if family := scipFamilyFor(root, filepath.Ext(file)); family != "" {
			families[family] = true
		}
	}
	if len(selection) > 0 && selection[0].forceTS {
		families["typescript"] = true
	}
	if len(selection) > 0 && selection[0].forceClang {
		families["clang"] = true
	}
	if len(families) == 0 {
		return nil, nil
	}
	// Reuse the AST walk itself. It handles ignored parent directories, nested
	// ignore files and negations; an indexer may include documents outside it.
	var files []string
	if len(selection) > 0 {
		files = selection[0].files
	} else {
		var err error
		files, err = collectFiles(root)
		if err != nil {
			return nil, []error{fmt.Errorf("SCIP file selection: %w", err)}
		}
	}
	allowedByFamily := make(map[string]map[string]bool)
	for _, file := range files {
		if excludeExts[strings.ToLower(filepath.Ext(file))] {
			continue
		}
		if rel, err := filepath.Rel(root, file); err == nil {
			family := scipFamilyFor(root, filepath.Ext(file))
			if family != "" {
				if allowedByFamily[family] == nil {
					allowedByFamily[family] = make(map[string]bool)
				}
				allowedByFamily[family][filepath.ToSlash(rel)] = true
			}
		}
	}
	entries := make(map[string]*parseCacheEntry)
	var failures []error
	familyNames := make([]string, 0, len(families))
	for family := range families {
		familyNames = append(familyNames, family)
	}
	sort.Strings(familyNames)
	for _, family := range familyNames {
		profile, err := scipProfileFor(root, family)
		if err != nil {
			failures = append(failures, fmt.Errorf("%s SCIP grammar: %w", family, err))
			continue
		}
		for rel := range allowedByFamily[family] {
			if !profile.supportsExt(strings.ToLower(filepath.Ext(rel))) {
				delete(allowedByFamily[family], rel)
			}
		}
		if len(allowedByFamily[family]) == 0 {
			// A valid profile may deliberately leave an extension to the syntax
			// parser. Its next edit changes the profile signature and reindexes.
			continue
		}
		data, err := runSCIPImage(ctx, root, graphCacheDir, family, allowedByFamily[family])
		if err != nil {
			failures = append(failures, fmt.Errorf("%s indexer: %w", family, err))
			continue
		}
		decoded, err := decodeSCIPWithProfiles(data, root, allowedByFamily[family], map[string]SCIPProfile{family: profile})
		if err != nil {
			failures = append(failures, fmt.Errorf("%s SCIP decoding: %w", family, err))
			continue
		}
		for path, entry := range decoded {
			if scipFamilyFor(root, filepath.Ext(path)) == family {
				entries[path] = entry
			}
		}
	}
	return entries, failures
}
