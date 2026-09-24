package ast

import (
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
)

type scipSelectionState struct {
	mu        sync.Mutex
	checkedAt time.Time
	enabled   bool
}

var scipSelectionStates sync.Map

func scipEnabledFor(projectDir string) bool {
	if projectDir == "" {
		return false
	}
	value, _ := scipSelectionStates.LoadOrStore(projectDir, &scipSelectionState{})
	state := value.(*scipSelectionState)
	state.mu.Lock()
	defer state.mu.Unlock()
	if time.Since(state.checkedAt) < time.Second {
		return state.enabled
	}
	state.enabled = config.ResolveAstSCIPEnabled(nil, config.LoadProjectConfig(projectDir))
	state.checkedAt = time.Now()
	return state.enabled
}

func refreshSCIPSelection(projectDir string) {
	if value, ok := scipSelectionStates.Load(projectDir); ok {
		state := value.(*scipSelectionState)
		state.mu.Lock()
		state.checkedAt = time.Time{}
		state.mu.Unlock()
	}
}

// scipFamilies is the intersection of Graphit's discovered source extensions and
// indexers that emit SCIP. A family names the graphit-scip image and its cache.
var scipFamilies = map[string]string{
	".go": "go",
	".ts": "typescript", ".tsx": "typescript", ".mts": "typescript", ".cts": "typescript",
	".js": "typescript", ".jsx": "typescript", ".mjs": "typescript", ".cjs": "typescript",
	".py": "python", ".pyi": "python",
	".java": "java", ".kt": "java", ".kts": "java",
	".c": "clang", ".h": "clang", ".cpp": "clang", ".hpp": "clang", ".cc": "clang", ".cxx": "clang", ".hxx": "clang", ".hh": "clang",
	".cs": "dotnet", ".vb": "dotnet", ".rs": "rust", ".rb": "ruby", ".dart": "dart", ".php": "php",
}

func scipFamilyFor(projectDir, ext string) string {
	if !scipEnabledFor(projectDir) {
		return ""
	}
	ext = strings.ToLower(ext)
	family := scipFamilies[ext]
	if family == "" {
		return ""
	}
	language := family
	switch ext {
	case ".js", ".jsx", ".mjs", ".cjs":
		language = "javascript"
	case ".kt", ".kts":
		language = "kotlin"
	case ".c", ".h":
		language = "c"
	case ".cpp", ".hpp", ".cc", ".cxx", ".hxx", ".hh":
		language = "cpp"
	case ".cs":
		language = "csharp"
	case ".vb":
		language = "visualbasic"
	}
	if !grammarFilterFor(projectDir).allows(language, "scip-"+family) {
		return ""
	}
	return family
}

func scipFamilyConfigured(projectDir string) bool {
	return scipEnabledFor(projectDir)
}

func scipConfigurationSignature(projectDir string) string {
	cfg := config.LoadProjectConfig(projectDir)
	signature := config.ResolveConfig("ast.scip.enabled", nil, cfg) + "\n" +
		config.ResolveAstSCIPVersion(nil, cfg)
	if !config.ResolveAstSCIPEnabled(nil, cfg) {
		return signature
	}
	return signature + "\n" + scipProfilesSignature(projectDir)
}
