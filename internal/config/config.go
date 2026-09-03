package config

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"sync"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

type ConfigMap = map[string]any

func AppDir() (string, error) {
	dir := brand.GlobalDir()
	if dir == "" {
		return "", fmt.Errorf("cannot resolve the global %s directory: no home directory", brand.Brand)
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	return dir, nil
}

const globalConfigFile = "config.json"

func globalConfigPath() (string, error) {
	dir, err := AppDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, globalConfigFile), nil
}

func LoadGlobalConfig() (ConfigMap, error) {
	path, err := globalConfigPath()
	if err != nil {
		return nil, err
	}

	data, err := os.ReadFile(path)
	if err != nil {
		if os.IsNotExist(err) {
			return make(ConfigMap), nil
		}
		return nil, fmt.Errorf("reading global config: %w", err)
	}

	var cfg ConfigMap
	if err := json.Unmarshal(data, &cfg); err != nil {
		return nil, fmt.Errorf("parsing global config: %w", err)
	}
	if cfg == nil {
		cfg = make(ConfigMap)
	}
	return cfg, nil
}

func SaveGlobalConfig(cfg ConfigMap) error {
	path, err := globalConfigPath()
	if err != nil {
		return err
	}

	for k, v := range cfg {
		if sec, ok := v.(map[string]any); ok && len(sec) == 0 {
			delete(cfg, k)
		}
	}

	data, err := json.MarshalIndent(cfg, "", "  ")
	if err != nil {
		return fmt.Errorf("serializing global config: %w", err)
	}

	return os.WriteFile(path, data, 0o600)
}

func GetConfigValue(cfg ConfigMap, dotKey string) (string, bool) {
	section, key, nested := splitKey(dotKey)

	if !nested {

		if val, ok := cfg[dotKey]; ok {
			if s, ok := val.(string); ok {
				return s, true
			}
		}
		return "", false
	}

	if sec, ok := cfg[section]; ok {
		if m, ok := sec.(map[string]any); ok {
			if val, ok := m[key]; ok {
				if s, ok := val.(string); ok {
					return s, true
				}
			}
		}
	}
	return "", false
}

func SetConfigValue(cfg ConfigMap, dotKey, value string) {
	section, key, nested := splitKey(dotKey)

	if !nested {
		cfg[dotKey] = value
		return
	}

	sec, ok := cfg[section]
	if !ok {
		cfg[section] = map[string]any{key: value}
		return
	}
	if m, ok := sec.(map[string]any); ok {
		m[key] = value
	} else {
		cfg[section] = map[string]any{key: value}
	}
}

func UnsetConfigValue(cfg ConfigMap, dotKey string) {
	section, key, nested := splitKey(dotKey)

	if !nested {
		delete(cfg, dotKey)
		return
	}

	if sec, ok := cfg[section]; ok {
		if m, ok := sec.(map[string]any); ok {
			delete(m, key)
			if len(m) == 0 {
				delete(cfg, section)
			}
		}
	}
}

func ListConfigEntries(cfg ConfigMap) [][2]string {
	var entries [][2]string
	for k, v := range cfg {
		switch val := v.(type) {
		case string:
			entries = append(entries, [2]string{k, val})
		case map[string]any:
			for subK, subV := range val {
				if s, ok := subV.(string); ok {
					entries = append(entries, [2]string{k + "." + subK, s})
				}
			}
		}
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i][0] < entries[j][0] })
	return entries
}

func GetGlobalConfigValue(dotKey string) (string, bool, error) {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return "", false, err
	}
	val, ok := GetConfigValue(cfg, dotKey)
	return val, ok, nil
}

func SetGlobalConfigValue(dotKey, value string) error {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return err
	}
	SetConfigValue(cfg, dotKey, value)
	return SaveGlobalConfig(cfg)
}

func UnsetGlobalConfigValue(dotKey string) error {
	cfg, err := LoadGlobalConfig()
	if err != nil {
		return err
	}
	UnsetConfigValue(cfg, dotKey)
	return SaveGlobalConfig(cfg)
}

var CompiledDefaults string

var (
	parsedDefaults ConfigMap
	defaultsOnce   sync.Once
)

func getCompiledDefaults() ConfigMap {
	defaultsOnce.Do(func() {
		parsedDefaults = make(ConfigMap)
		if CompiledDefaults == "" {
			return
		}
		pairs := strings.Split(CompiledDefaults, ",")
		for _, pair := range pairs {
			parts := strings.SplitN(pair, "=", 2)
			if len(parts) == 2 {
				SetConfigValue(parsedDefaults, strings.TrimSpace(parts[0]), strings.TrimSpace(parts[1]))
			}
		}
	})
	return parsedDefaults
}

// ConfigEnvVar returns the environment variable that supplies a config key: the brand prefix,
// the key upper-cased, dots turned into underscores. `hub.bucket` becomes GRAPHIT_HUB_BUCKET.
//
// This is THE derivation — ResolveConfig calls it, so every key in the codebase gets an
// environment variable from here with no registration step. Anything that needs to NAME the
// variable — help text, an error message, documentation, a container image — calls this rather
// than rebuilding the rule, because a second implementation of a string transformation is a
// second thing to keep in step and nothing would report the day they diverged.
func ConfigEnvVar(key string) string {
	return brand.EnvPrefix() + "_" + strings.ToUpper(strings.ReplaceAll(strings.TrimSpace(key), ".", "_"))
}

func ResolveConfig(key string, inlineCfg, projectCfg ConfigMap) string {

	if inlineCfg != nil {
		if val, ok := GetConfigValue(inlineCfg, key); ok && val != "" {
			return val
		}
	}

	if val := os.Getenv(ConfigEnvVar(key)); val != "" {
		return val
	}

	if projectCfg != nil {
		if val, ok := GetConfigValue(projectCfg, key); ok && val != "" {
			return val
		}
	}

	if val, ok, _ := GetGlobalConfigValue(key); ok && val != "" {
		return val
	}

	defs := getCompiledDefaults()
	if val, ok := GetConfigValue(defs, key); ok && val != "" {
		return val
	}

	return ""
}

// FallbackIDE and FallbackCLI are the last resort of every IDE/CLI resolution: what the framework
// uses when no flag, no inline map, no environment variable, no project lockfile, no global config
// and no compiled-in default has an opinion.
//
// They are constants, and named, because this value is read at the bottom of FIVE different
// resolution paths — ResolveIDE, resolveAmbientIDE, ResolveCLI, the unified UI server, and
// CLIForIDE's pairing. As three separate string literals they could be changed one at a time, and
// the result would not fail: the paths would simply disagree about what the default is, which is
// the kind of divergence nothing reports.
//
// The two are kept consistent by CLIForIDE: FallbackIDE's paired CLI IS FallbackCLI, and
// TestFallbackIDEAndCLIAgree fails if that stops being true.
const (
	FallbackIDE = "opencode"
	FallbackCLI = "opencode"
)

func ResolveIDE(flagValue string, inlineCfg, projectCfg ConfigMap) string {

	if flagValue != "" {
		return flagValue
	}

	if val := ResolveConfig("ide", inlineCfg, projectCfg); val != "" {
		return val
	}

	return FallbackIDE
}

func ResolveProjectIDE(flagValue string, inlineCfg, projectCfg ConfigMap, lockfileIDEs []string) string {

	if flagValue != "" {
		return flagValue
	}

	if inlineCfg != nil {
		if val, ok := GetConfigValue(inlineCfg, "ide"); ok && val != "" {
			return val
		}
	}

	if projectCfg != nil {
		if val, ok := GetConfigValue(projectCfg, "ide"); ok && val != "" {
			return val
		}
	}

	resolved := resolveAmbientIDE()

	if len(lockfileIDEs) > 0 {
		for _, registered := range lockfileIDEs {
			if strings.EqualFold(registered, resolved) {
				return resolved
			}
		}

		return lockfileIDEs[0]
	}

	return resolved
}

func resolveAmbientIDE() string {
	envKey := brand.EnvPrefix() + "_IDE"
	if val := os.Getenv(envKey); val != "" {
		return val
	}

	if val, ok, _ := GetGlobalConfigValue("ide"); ok && val != "" {
		return val
	}

	defs := getCompiledDefaults()
	if val, ok := GetConfigValue(defs, "ide"); ok && val != "" {
		return val
	}

	return FallbackIDE
}

func DefaultIDE() string {
	return ResolveIDE("", nil, nil)
}

func ResolveCLI(flagValue string, inlineCfg, projectCfg ConfigMap, resolvedIDE string) string {

	if flagValue != "" {
		return flagValue
	}

	if val := ResolveConfig("cli", inlineCfg, projectCfg); val != "" {
		return val
	}

	if resolvedIDE != "" {
		if cli := CLIForIDE(resolvedIDE); cli != "" {
			return cli
		}
	}

	return FallbackCLI
}

func DefaultCLI() string {
	return ResolveCLI("", nil, nil, DefaultIDE())
}

func CLIForIDE(ide string) string {
	switch strings.ToLower(ide) {
	case "antigravity":
		return "agy"
	case "gemini", "gemini-code":
		return "gemini"
	case "claude", "claude-code":
		return "claude"
	case "cursor":
		return "cursor-agent"
	case "codex":
		return "codex"
	case "opencode":
		return "opencode"
	case "kiro":
		return "kiro-cli"
	default:
		return ""
	}
}

func IsSetupDone() bool {
	path, err := globalConfigPath()
	if err != nil {
		return false
	}
	_, err = os.Stat(path)
	return err == nil
}

// ResolveSearchRerank reads whether the cross-encoder reranking stage is enabled.
//
// DEFAULT FALSE, and the default is what gates a 1.04 GiB download: the reranker's model is fetched
// only when this is true AND it is not already on disk. Nothing downloads it at `setup`, and a
// user who leaves this off never pays for it.
//
// What it buys, and why it is not on: retrieval already fuses the dense and the BM25 channel with
// the engine's own reciprocal-rank-fusion. A cross-encoder is the other family of reranker — it
// reads each (query, candidate) pair and judges relevance directly, which is what LanceDB's
// documentation points to for quality beyond fusion. It costs a second model and inference on the
// query path, so it is a decision rather than a default.
func ResolveSearchRerank(inlineCfg, projectCfg ConfigMap) bool {
	// Absent means false, which is the same convention ast.index_docs uses: a boolean module
	// switch that is off unless the operator wrote "true".
	return strings.EqualFold(ResolveConfig("search.rerank", inlineCfg, projectCfg), "true")
}

// SearchRerank is ResolveSearchRerank with no inline or project configuration.
func SearchRerank() bool { return ResolveSearchRerank(nil, nil) }

// DefaultDocsDir is the documentation tree the knowledge module indexes when
// knowledge.docs_dir says nothing.
//
// This used to be "." — the whole project — which made the wiki index every
// indexable file in the repository: vendored markdown, generated JSON, IDE
// adapter directories, node_modules leftovers the ignore file did not name. The
// wiki is documentation, and documentation lives in a directory; a project whose
// docs are elsewhere says so with knowledge.docs_dir.
const DefaultDocsDir = "docs"

// ResolveDocsDir returns the documentation tree, relative to the project root.
// Override it with knowledge.docs_dir — inline, GRAPHIT_KNOWLEDGE_DOCS_DIR,
// project lockfile, global config, in that order of precedence. Set it to "."
// to restore the pre-default behaviour of indexing the whole project.
func ResolveDocsDir(inlineCfg, projectCfg ConfigMap) string {
	val := ResolveConfig("knowledge.docs_dir", inlineCfg, projectCfg)
	if val != "" {
		return val
	}
	return DefaultDocsDir
}

// ResolveKnowledgeIncludeReadme reports whether the project root's README is
// indexed into the knowledge wiki on top of whatever ResolveDocsDir returns.
//
// It is on by default: the README is the one document every repository has and
// the one a reader reaches for first, and scoping the wiki to docs/ would
// otherwise have dropped it. Set knowledge.include_readme to "false" to index
// only the docs tree.
func ResolveKnowledgeIncludeReadme(inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("knowledge.include_readme", inlineCfg, projectCfg)
	return !strings.EqualFold(val, "false")
}

// ResolveHubIcebugReverseEdges reports whether Hub AST publications include the
// reverse CSR. Only an explicit false disables the default.
func ResolveHubIcebugReverseEdges(inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("hub.icebug.reverse_edges", inlineCfg, projectCfg)
	return !strings.EqualFold(val, "false")
}

// DefaultASTQueriesDir is the project-level grammar directory when
// ast.queries_dir says nothing: where it has always been, inside the brand
// directory.
//
// It is a tracked location. Only brand.RuntimeSubdir is gitignored, so a grammar
// override committed here reaches every other checkout without configuring
// anything. The key remains for a project that would rather keep its grammars
// beside its other tooling.
func DefaultASTQueriesDir() string {
	return filepath.Join(brand.DotDir(), "ast", "queries")
}

// ResolveASTQueriesDir returns the directory holding the project's own grammar
// query files, relative to the project root. Override it with ast.queries_dir —
// inline, GRAPHIT_AST_QUERIES_DIR, project lockfile, global config, in that
// order of precedence.
//
// The value is a path, not a list: a project has one grammar directory, and the
// levels above it (~/.graphit/ast/queries and the runtime's) are unaffected.
func ResolveASTQueriesDir(inlineCfg, projectCfg ConfigMap) string {
	val := strings.TrimSpace(ResolveConfig("ast.queries_dir", inlineCfg, projectCfg))
	if val == "" {
		return DefaultASTQueriesDir()
	}
	return filepath.Clean(filepath.FromSlash(val))
}

// ResolveASTGrammarsBlacklist returns the raw value of ast.grammars_blacklist:
// a comma-separated list of grammars the AST index must not use. Resolved
// inline, GRAPHIT_AST_GRAMMARS_BLACKLIST, project lockfile, global config, in
// that order of precedence.
//
// The value is returned unparsed. Splitting it belongs to the AST package, which
// also uses the raw string as the staleness signature of its per-project filter
// cache — a parsed form would have to be re-serialised to serve as one.
func ResolveASTGrammarsBlacklist(inlineCfg, projectCfg ConfigMap) string {
	return strings.TrimSpace(ResolveConfig("ast.grammars_blacklist", inlineCfg, projectCfg))
}

// ResolveASTGrammarsWhitelist returns the raw value of ast.grammars_whitelist:
// a comma-separated list of the only grammars the AST index may use. Empty —
// the default — means every grammar, not none.
//
// See ResolveASTGrammarsBlacklist for the precedence and for why the value is
// returned unparsed.
func ResolveASTGrammarsWhitelist(inlineCfg, projectCfg ConfigMap) string {
	return strings.TrimSpace(ResolveConfig("ast.grammars_whitelist", inlineCfg, projectCfg))
}

// ResolveAstIndexDocs reports whether the AST pipeline indexes the knowledge
// module's documentation tree.
//
// It is off by default: the docs tree belongs to the knowledge wiki, which
// chunks, links and ranks prose far better than a code graph can, and indexing
// it twice buys nothing but a Heading node per section. Set ast.index_docs to
// "true" when the documentation is code-shaped enough to want structural
// queries over it — .proto or .graphql schemas kept under docs/, say.
func ResolveAstIndexDocs(inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("ast.index_docs", inlineCfg, projectCfg)
	return strings.EqualFold(val, "true")
}

// ResolveTaskPrefix returns the namespace that holds authoritative task tables.
// It follows the ordinary configuration precedence and is nested under the Hub
// prefix when object storage is configured.
func ResolveTaskPrefix(inlineCfg, projectCfg ConfigMap) string {
	value := normalizePrefix(ResolveConfig("task.prefix", inlineCfg, projectCfg))
	if value == "" {
		return "tasks"
	}
	return value
}

// ResolveDreamReportsDir returns the directory holding dream session reports,
// relative to the project root. Override it with dream.reports_dir — inline,
// GRAPHIT_DREAM_REPORTS_DIR, project lockfile, global config, in that order of
// precedence.
//
// The default lives under the project's ignored runtime tree. Task intent belongs
// to Graphit Task's authoritative tables, while dream reports, sentinels and the
// per-developer read marker are generated output. This key lets a project move the
// whole vault to a versionable location such as docs/ when publication is explicit.
// An empty return means "unset": the caller applies the default, which lives in
// the dream package because it depends on the brand directory and this package
// stays below brand deliberately.
func ResolveDreamReportsDir(inlineCfg, projectCfg ConfigMap) string {
	val := strings.TrimSpace(ResolveConfig("dream.reports_dir", inlineCfg, projectCfg))
	if val == "" {
		return ""
	}
	return filepath.Clean(filepath.FromSlash(val))
}

// LoadProjectConfig reads the `config` object out of a project's lockfile.
//
// It duplicates a sliver of hub.LoadLockfile on purpose: hub imports ast, so ast
// cannot import hub, and the AST side needs project configuration to decide
// whether the docs tree is its business. Anything richer about a lockfile still
// belongs to the hub package. A missing or malformed lockfile resolves to nil,
// which ResolveConfig treats as "nothing set here" and falls through.
func LoadProjectConfig(projectDir string) ConfigMap {
	data, err := os.ReadFile(filepath.Join(projectDir, brand.LockFileName()))
	if err != nil {
		return nil
	}
	var lf struct {
		Config ConfigMap `json:"config"`
	}
	if err := json.Unmarshal(data, &lf); err != nil {
		return nil
	}
	return lf.Config
}

var defaultKnowledgeExtensions = []string{
	".md", ".markdown", ".mdx",
	".txt", ".adoc", ".rst",
	".puml", ".plantuml",
	".yaml", ".yml", ".json",
	".proto", ".graphql", ".gql",
	".wsdl", ".xml",
}

// ResolveKnowledgeExtensions returns the set of file extensions the knowledge
// wiki should index. Configurable via knowledge.extensions (comma-separated).
// Falls back to the built-in default set.
func ResolveKnowledgeExtensions(inlineCfg, projectCfg ConfigMap) map[string]bool {
	val := ResolveConfig("knowledge.extensions", inlineCfg, projectCfg)

	var exts []string
	if val != "" {
		for _, e := range strings.Split(val, ",") {
			e = strings.TrimSpace(strings.ToLower(e))
			if e == "" {
				continue
			}
			if !strings.HasPrefix(e, ".") {
				e = "." + e
			}
			exts = append(exts, e)
		}
	}

	if len(exts) == 0 {
		exts = defaultKnowledgeExtensions
	}

	m := make(map[string]bool, len(exts))
	for _, e := range exts {
		m[e] = true
	}
	return m
}

// OptInModules are the modules that are OFF unless explicitly enabled. Every other module is on
// by default, so `modules.<name>` reads as an enabled flag whose absence means "yes".
//
// Both entries here run something the user did not ask for at the moment they would run it:
// dream works overnight against an agent CLI, and daemon_ui makes a background process hold a
// port and serve a web application. Defaults that surprising have to be chosen, not inherited.
var OptInModules = []string{
	"dream",
	DaemonUIModule,
}

func IsModuleDisabled(module string, inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("modules."+module, inlineCfg, projectCfg)

	if strings.EqualFold(val, "false") {
		return true
	}

	if strings.EqualFold(val, "true") {
		return false
	}

	if isOptInModule(module) {
		return true
	}

	return false
}

func ResolveIndexSource(inlineCfg, projectCfg ConfigMap) bool {
	val := ResolveConfig("ast.index_source", inlineCfg, projectCfg)
	return !strings.EqualFold(val, "false")
}

// defaultProjectActivityWindow is how long a registered project may stay
// touched-recently before the daemon parks it: stops its filesystem watch,
// embedding loop and dream runner rather than keeping them alive for a
// project nobody is working on.
const defaultProjectActivityWindow = 30 * time.Minute

// ResolveProjectActivityWindow returns how recently a project must have
// changed to remain — or become — fully supervised by the daemon.
// Configurable via daemon.activity_window (a Go duration string, e.g. "15m").
// Set to "0" to disable parking entirely: every registered project stays
// supervised for as long as it stays registered, which is the pre-parking
// behavior. An unset or invalid value falls back to the 30-minute default.
func ResolveProjectActivityWindow(inlineCfg, projectCfg ConfigMap) time.Duration {
	val := ResolveConfig("daemon.activity_window", inlineCfg, projectCfg)
	if val == "" {
		return defaultProjectActivityWindow
	}
	d, err := time.ParseDuration(val)
	if err != nil || d < 0 {
		return defaultProjectActivityWindow
	}
	return d
}

// defaultWikiVersionRetention is how long a superseded wiki index version survives.
//
// FIFTEEN MINUTES IS A MARGIN FOR IN-FLIGHT READERS, not a backup window, and the difference is
// what makes this configurable. Lance is MVCC: a reader answers from the snapshot it opened, so
// pruning a version somebody is still reading takes their query down. Fifteen minutes comfortably
// exceeds the longest read this process performs, and beyond that a retained version costs a full
// superseded copy on disk for a time travel the knowledge wiki never performs — it is rebuilt from
// `docs/`, so its recovery path is the sources.
//
// A store with NO second copy of its data is the case that wants a longer window: there the history
// IS the recovery path, and the retention is the length of the safety net. Proved restorable in
// internal/lancestore (TestAnEarlierVersionIsRestorableAfterADestructiveWrite).
const defaultWikiVersionRetention = 15 * time.Minute

// ResolveWikiVersionRetention returns how long a superseded wiki index version is kept before a
// maintenance pass may reclaim it. Configurable via wiki.version_retention, a Go duration string.
//
// A value BELOW ONE SECOND is rejected in favour of the default, and that is measured rather than
// defensive: the engine prunes nothing at all for a sub-second window — it reports zero old
// versions while they plainly exist — so accepting "1ms" would silently disable pruning while
// looking like an aggressive setting.
func ResolveWikiVersionRetention(inlineCfg, projectCfg ConfigMap) time.Duration {
	val := ResolveConfig("wiki.version_retention", inlineCfg, projectCfg)
	if val == "" {
		return defaultWikiVersionRetention
	}
	d, err := time.ParseDuration(val)
	if err != nil || d < time.Second {
		return defaultWikiVersionRetention
	}
	return d
}

// WikiVersionRetention is ResolveWikiVersionRetention against the resolved configuration.
func WikiVersionRetention() time.Duration { return ResolveWikiVersionRetention(nil, nil) }

// defaultMemoryVersionRetention is how long a superseded version of the MEMORY STORE survives.
//
// THIRTY DAYS, AND IT IS A DIFFERENT KIND OF NUMBER FROM THE WIKI'S FIFTEEN MINUTES. The wiki's
// retention is a margin for in-flight readers: its index is derived, so a pruned version costs
// nothing but a rebuild. The memory store is the only copy of what it holds — the markdown raw
// store that used to be its second copy is being retired — so its version history IS the recovery
// path, and the retention is the LENGTH OF THE SAFETY NET.
//
// That is the decision D2 rests on: version retention was accepted as memory's recovery mechanism
// only because a rollback was proved to work through `lancestore`. Reusing the wiki's fifteen
// minutes here would have honoured the letter of that and broken it in fact — a bad pass discovered
// the next morning would find nothing to roll back to.
//
// Thirty days rather than unbounded because a retained version holds a full superseded copy of the
// table, and "keep everything forever" is a disk-growth policy pretending to be a safety policy. A
// month is long enough for a mistake to be noticed by a person.
const defaultMemoryVersionRetention = 30 * 24 * time.Hour

// ResolveMemoryVersionRetention returns how long a superseded version of the memory store is kept.
// Configurable via memory.version_retention, a Go duration string.
//
// The one-second floor is the same measured constraint the wiki's has: the engine prunes nothing at
// all below a second, so honouring a smaller value would silently disable pruning while reading as
// the most aggressive setting available.
func ResolveMemoryVersionRetention(inlineCfg, projectCfg ConfigMap) time.Duration {
	val := ResolveConfig("memory.version_retention", inlineCfg, projectCfg)
	if val == "" {
		return defaultMemoryVersionRetention
	}
	d, err := time.ParseDuration(val)
	if err != nil || d < time.Second {
		return defaultMemoryVersionRetention
	}
	return d
}

// MemoryVersionRetention is ResolveMemoryVersionRetention against the resolved configuration.
func MemoryVersionRetention() time.Duration { return ResolveMemoryVersionRetention(nil, nil) }

// ParseGrammarOverrides parses a comma-separated grammar override string
// into a map[string]string. Format: ".ext=grammar-name,.ext2=grammar-name2".
// Returns nil if s is empty or contains no valid pairs.
func ParseGrammarOverrides(s string) map[string]string {
	if s == "" {
		return nil
	}
	m := make(map[string]string)
	for _, pair := range strings.Split(s, ",") {
		parts := strings.SplitN(strings.TrimSpace(pair), "=", 2)
		if len(parts) != 2 {
			continue
		}
		ext := strings.TrimSpace(parts[0])
		name := strings.TrimSpace(parts[1])
		if ext == "" || name == "" {
			continue
		}
		if !strings.HasPrefix(ext, ".") {
			ext = "." + ext
		}
		m[strings.ToLower(ext)] = name
	}
	if len(m) == 0 {
		return nil
	}
	return m
}

// ResolveGrammarOverrides returns the grammar override map from config.
// Configurable via ast.grammar (comma-separated .ext=grammar-name pairs).
// Uses the standard resolution chain: inline → env → project → global → defaults.
// Returns nil if no overrides are configured.
func ResolveGrammarOverrides(inlineCfg, projectCfg ConfigMap) map[string]string {
	val := ResolveConfig("ast.grammar", inlineCfg, projectCfg)
	return ParseGrammarOverrides(val)
}

// MergeGrammarOverrides merges base overrides with higher-priority overrides.
// Priority entries overwrite base entries for the same extension.
// Returns nil if both inputs are nil.
func MergeGrammarOverrides(base, priority map[string]string) map[string]string {
	if base == nil && priority == nil {
		return nil
	}
	if base == nil {
		return priority
	}
	if priority == nil {
		return base
	}
	merged := make(map[string]string, len(base)+len(priority))
	for k, v := range base {
		merged[k] = v
	}
	for k, v := range priority {
		merged[k] = v
	}
	return merged
}

// ParseClusterPathMap parses a comma-separated cluster path map string
// into a map[string]string. Format: "path1=cluster1,path2=cluster2".
// Paths are directory prefixes (trailing slash added if missing).
// Returns nil if s is empty or contains no valid pairs.
func ParseClusterPathMap(s string) map[string]string {
	if s == "" {
		return nil
	}
	m := make(map[string]string)
	for _, pair := range strings.Split(s, ",") {
		parts := strings.SplitN(strings.TrimSpace(pair), "=", 2)
		if len(parts) != 2 {
			continue
		}
		path := strings.TrimSpace(parts[0])
		cluster := strings.TrimSpace(parts[1])
		if path == "" || cluster == "" {
			continue
		}
		// Normalize path: ensure trailing slash for prefix matching
		path = strings.TrimRight(path, "/") + "/"
		m[path] = cluster
	}
	if len(m) == 0 {
		return nil
	}
	return m
}

// ResolveClusterPathMap returns the cluster path map from config.
// Configurable via ast.cluster_map (comma-separated path=cluster pairs).
// Returns nil if no cluster map is configured.
func ResolveClusterPathMap(inlineCfg, projectCfg ConfigMap) map[string]string {
	val := ResolveConfig("ast.cluster_map", inlineCfg, projectCfg)
	return ParseClusterPathMap(val)
}

// MergeClusterPathMap merges base cluster path map with higher-priority map.
// Priority entries overwrite base entries for the same path prefix.
// Returns nil if both inputs are nil.
func MergeClusterPathMap(base, priority map[string]string) map[string]string {
	if base == nil && priority == nil {
		return nil
	}
	if base == nil {
		return priority
	}
	if priority == nil {
		return base
	}
	merged := make(map[string]string, len(base)+len(priority))
	for k, v := range base {
		merged[k] = v
	}
	for k, v := range priority {
		merged[k] = v
	}
	return merged
}

func HubRepoDirPath() (string, error) {
	dir, err := AppDir()
	if err != nil {
		return "", err
	}
	return filepath.Join(dir, "hub"), nil
}

func splitKey(dotKey string) (section, key string, nested bool) {
	parts := strings.SplitN(dotKey, ".", 2)
	if len(parts) == 2 && parts[0] != "" && parts[1] != "" {
		return parts[0], parts[1], true
	}
	return dotKey, "", false
}

func isOptInModule(module string) bool {
	for _, m := range OptInModules {
		if strings.EqualFold(m, module) {
			return true
		}
	}
	return false
}
