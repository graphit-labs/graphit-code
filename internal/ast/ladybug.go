package ast

import (
	"context"
	"encoding/json"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"sync"
	"time"

	lbug "github.com/LadybugDB/go-ladybug"

	"github.com/graphit-labs/graphit-code/internal/config"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
	"github.com/graphit-labs/graphit-code/internal/store"
)

var (
	reOnCreateSet = regexp.MustCompile(`(?i)\bON\s+CREATE\s+SET\b`)
	reOnMatchSet  = regexp.MustCompile(`(?i)\bON\s+MATCH\s+SET\b`)
	reTypeParen   = regexp.MustCompile(`\btype\(`)
	reParamUsage  = regexp.MustCompile(`\$(\w+)`)

	// A label predicate — `WHERE n:Function`, `AND (n:Struct OR n:Method)` — is
	// Neo4j syntax that LadybugDB's parser rejects outright, so it is rewritten to
	// `label(n) = 'Function'`, which is the form that parses. The prefix group
	// swallows an optional NOT and any opening parens, so the first alternative of
	// a parenthesised group is rewritten like every other one.
	reLabelPredicate = regexp.MustCompile(`(?i)(\b(?:WHERE|AND|OR|XOR)\s+(?:NOT\s+)?(?:\(\s*)*)([A-Za-z_]\w*):([A-Za-z_]\w*)`)

	// A label in a pattern position — `(n:Label)`, `(:Label)`, `-[r:REL]->`,
	// `-[:REL]->` — is backticked so a label colliding with a Cypher keyword still
	// parses. Position is the entire test: what follows the colon is a label
	// because of where it sits, not because it appears on some list.
	//
	// There deliberately is no list. Labels are declared by `graph_label:` in query
	// files that are loaded at runtime from three directories, so a user grammar can
	// name a label this binary has never heard of; a hardcoded set cannot be right
	// and does not fail loudly when it drifts — it just stops escaping.
	rePatternLabel = regexp.MustCompile(`([(\[|]\s*\w*\s*:\s*)([A-Za-z_]\w*)`)

	// In a relationship type alternation only the first alternative carries the
	// colon — `-[r:CALLS|IMPORTS]->` — so rePatternLabel escapes CALLS and walks
	// past IMPORTS, which then breaks if it collides with a keyword. These two find
	// the rest of the alternatives.
	//
	// reRelTypeList is deliberately narrow: it only accepts a bracket whose content
	// opens with an optional variable and a colon, which is what a relationship
	// pattern looks like. A list comprehension — `[x IN xs | x]` — also has a pipe
	// inside brackets and must be left alone.
	reRelTypeList = regexp.MustCompile(`^\[\s*\w*\s*:`)
	reRelBracket  = regexp.MustCompile(`\[[^\[\]]*\]`)
	reRelAltType  = regexp.MustCompile(`([|]\s*:?\s*)([A-Za-z_]\w*)`)

	// Statements answered with RETURN 1 because the engine has neither secondary
	// indexes nor constraints. Nothing in this codebase emits them any more —
	// CreateGraphSchema did, and every one of its statements landed here, which is
	// what made the whole function dead — so what is left to catch is a hand-written
	// query in the Neo4j idiom. Anchored at the start so it matches a statement, not
	// the mention of one.
	reDDLNoop = regexp.MustCompile(`(?is)^\s*CREATE\s+(CONSTRAINT|INDEX)\b`)

	reLabelsIndex = regexp.MustCompile(`\blabels\((\w+)\)\[0\]`)

	// "Binder exception: Table Import does not exist." — the engine's way of saying
	// the label or relationship type is not in this graph.
	reMissingTable = regexp.MustCompile(`Table (\S+?) does not exist`)

	// "Binder exception: Cannot find property line_number for e."
	reMissingProperty = regexp.MustCompile(`Cannot find property (\S+?) for \S+`)
)

// mapOutsideStrings applies fn to each stretch of q that sits outside a quoted
// literal, leaving the literals byte for byte intact.
//
// Every rewrite below is a regex over Cypher syntax, and a string can legitimately
// contain the shapes those regexes look for: a uid reads `internal/x/y.go::Apply`,
// and a search term can be the text `(n:Function)`. Rewriting inside a literal
// changes what the query asks for, silently.
func mapOutsideStrings(q string, fn func(string) string) string {
	var out, seg strings.Builder
	var quote byte

	for i := 0; i < len(q); i++ {
		c := q[i]

		if quote != 0 {
			out.WriteByte(c)
			if c == '\\' && i+1 < len(q) {
				i++
				out.WriteByte(q[i])
				continue
			}
			if c == quote {
				quote = 0
			}
			continue
		}

		if c == '\'' || c == '"' {
			out.WriteString(fn(seg.String()))
			seg.Reset()
			quote = c
			out.WriteByte(c)
			continue
		}

		seg.WriteByte(c)
	}

	out.WriteString(fn(seg.String()))
	return out.String()
}

type LadybugConfig struct {
	// StoreDir is the global store directory of the graph — where graph.icebug/, search.lance/,
	// manifests and shards live. The catalog itself is ALWAYS in-memory; there is no file-based
	// mode anymore, so this is the only location a config needs.
	StoreDir string

	// IcebugDir is the directory holding the icebug bundle (graph.icebug) for a project, or
	// the local catalog cache dir (schema.cypher + icebug.json) for a Hub context over S3.
	// It is what the in-memory catalog is mounted from.
	IcebugDir string

	ReadOnly bool
}

// LadybugConfigFor is the graph store of one project.
//
// The local graph is icebug filesystem on-the-fly, mounted in-memory. No ladybugdb file exists;
// the bundle lives at ASTProjectIcebugDir and the catalog is :memory: built from schema.cypher.
// See docs/architecture/storage_layout.md.
func LadybugConfigFor(projectDir string) LadybugConfig {
	if override := os.Getenv("LADYBUGDB_PATH"); override != "" {
		return LadybugConfig{StoreDir: filepath.Dir(override), IcebugDir: override}
	}
	return LadybugConfig{
		StoreDir:  store.ASTProjectDir(projectDir),
		IcebugDir: store.ASTProjectIcebugDir(projectDir),
	}
}

// DefaultLadybugConfig is LadybugConfigFor the working directory. Prefer
// LadybugConfigFor wherever the project is known.
func DefaultLadybugConfig() LadybugConfig {
	wd, _ := os.Getwd()
	return LadybugConfigFor(wd)
}

func LadybugConfigForContext(name string) LadybugConfig {
	wd, _ := os.Getwd()
	return LadybugConfigForContextIn(wd, name)
}

// LadybugConfigForContextIn resolves a context — or the project's own graph, for an
// empty name — for a named project rather than the working directory.
//
// Every context is mounted the same way a project is: an in-memory catalog over an
// icebug bundle. For a Hub context the bundle lives on S3 and the local dir is the
// mount cache holding schema.cypher + icebug.json; for a local or linked context it
// is the filesystem bundle itself.
func LadybugConfigForContextIn(projectDir, name string) LadybugConfig {
	if name == "" || name == "__project__" {
		return LadybugConfigFor(projectDir)
	}
	if override := os.Getenv("LADYBUGDB_PATH"); override != "" {
		return LadybugConfig{StoreDir: filepath.Dir(override), IcebugDir: override}
	}
	return LadybugConfig{
		StoreDir:  store.ASTContextDirIn(projectDir, name),
		IcebugDir: store.ASTContextIcebugDirIn(projectDir, name),
	}
}

type LadybugBackend struct {
	cfg    LadybugConfig
	db     *lbug.Database
	conn   *lbug.Connection
	mu     sync.Mutex
	Logger *slog.Logger

	// bufferPool is the ceiling this handle was actually opened with, recorded so a
	// failure caused by that ceiling can name it. An error that says "the buffer pool
	// is full" while the machine has tens of gigabytes free is unreadable without it.
	bufferPool uint64

	stmtCache map[string]*lbug.PreparedStatement

	// canonical holds the v2 manifest of a CANONICAL icebug catalog — real node tables per
	// label and one rel table per (type, from, to) pair. Loaded from icebug.json beside the
	// mounted database at connect; nil means the catalog is the folded layout.
	canonical *ladybug.CanonicalManifest
}

// BufferPoolBytes is the buffer-pool ceiling this handle was opened with, or 0 before it
// is connected.
func (k *LadybugBackend) BufferPoolBytes() uint64 {
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.bufferPool
}

func (k *LadybugBackend) log() *slog.Logger { return slogutil.Resolve(k.Logger) }

func NewLadybugDB(cfg LadybugConfig) *LadybugBackend {
	return &LadybugBackend{cfg: cfg, stmtCache: make(map[string]*lbug.PreparedStatement)}
}

func NewLadybugDBReadOnly(cfg LadybugConfig) *LadybugBackend {
	cfg.ReadOnly = true
	return &LadybugBackend{cfg: cfg, stmtCache: make(map[string]*lbug.PreparedStatement)}
}

// dbOpenAttempts and dbOpenBackoff bound the retry of a failed open.
//
// Opening is not reliably repeatable while another process writes to the same
// database: a reader can land in the window where the writer is checkpointing
// the file, and the engine reports that as an opaque status code — its C API has
// no error message channel for open at all, so every cause arrives as the same
// "failed to open database with status 1". Measured against a writer committing
// and checkpointing in a loop, 14 of 80 concurrent read-only opens failed that
// way, and the same open succeeded milliseconds later.
//
// Sleeps total ~750 ms across the attempts, which is the budget a read tool can
// spend without looking hung.
const (
	dbOpenAttempts = 5
	dbOpenBackoff  = 50 * time.Millisecond
)

func (k *LadybugBackend) connect() error {
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.connectLocked()
}

// connectLocked opens the in-memory catalog, retrying a transient failure. The caller
// must hold k.mu.
//
// A failure is deliberately NOT remembered. This used to run inside a sync.Once
// that stored the error in a field, so one open landing in a writer's
// checkpoint window poisoned the backend for the rest of its life: every later
// call on the same instance returned the stale error without retrying.
func (k *LadybugBackend) connectLocked() error {
	if k.conn != nil {
		return nil
	}

	var err error
	for attempt := 1; attempt <= dbOpenAttempts; attempt++ {
		if err = k.openOnce(); err == nil {
			if attempt > 1 {
				k.log().Debug("ladybug: opened after retry",
					"catalog", k.cfg.IcebugDir, "readonly", k.cfg.ReadOnly, "attempts", attempt)
			}
			return nil
		}
		if attempt < dbOpenAttempts {
			time.Sleep(dbOpenBackoff << (attempt - 1))
		}
	}
	k.log().Error("ladybug: failed to open in-memory catalog",
		"icebug_dir", k.cfg.IcebugDir, "readonly", k.cfg.ReadOnly, "attempts", dbOpenAttempts, "error", err)
	return err
}

// prepareRemoteAccessLocked loads the S3 filesystem extension and hands it credentials.
//
// EVERY FAILURE IS A WARNING, not an error, and that is the deliberate part: the overwhelmingly
// common store is local, and refusing to open one because an extension the launcher payload did
// not carry is missing would break every local project to serve the mounted case. A mounted store
// whose extension is absent fails later, on the first read, with the engine naming the URI it
// could not open — which is the message that actually points at the cause.
//
// The caller must hold k.mu.
func (k *LadybugBackend) prepareRemoteAccessLocked() {
	stmt, err := ladybug.ExtensionLoadStatement(ladybug.ExtHTTPFS)
	if err != nil {
		k.log().Debug("remote graph access unavailable", "reason", err,
			"impact", "a mounted context cannot be read; a local one is unaffected")
		return
	}
	if err := k.execQueryLocked(stmt); err != nil {
		k.log().Warn("loading the S3 filesystem extension failed", "error", err)
		return
	}

	cfg := config.HubS3Config()
	if !cfg.Configured() {
		return
	}
	creds := resolvedLadybugS3Credentials(cfg)
	for _, s := range ladybug.S3ConfigStatements(creds) {
		if err := k.execQueryLocked(s); err != nil {
			// The statement carries a secret in some cases, so only the failure is logged.
			k.log().Warn("configuring S3 access for the graph engine failed")
			return
		}
	}
}

// loadCanonicalManifestLocked reads icebug.json and adopts it
// when it describes a CANONICAL catalog. Absent file -> no icebug manifest.
func (k *LadybugBackend) loadCanonicalManifestLocked() error {
	candidates := []string{
		filepath.Join(k.cfg.IcebugDir, ladybug.IcebugManifestFile),
		filepath.Join(k.cfg.IcebugDir, "graph.icebug", ladybug.IcebugManifestFile),
	}
	for _, p := range candidates {
		raw, err := os.ReadFile(p)
		if err != nil {
			if os.IsNotExist(err) {
				continue
			}
			return fmt.Errorf("icebug: read manifest: %w", err)
		}
		var man ladybug.CanonicalManifest
		if err := json.Unmarshal(raw, &man); err != nil {
			return fmt.Errorf("icebug: parse manifest: %w", err)
		}
		if man.Format != "icebug-canonical" || !man.Finished {
			return fmt.Errorf("icebug: unsupported manifest format %q (only icebug-canonical v2+ supported)", man.Format)
		}
		k.canonical = &man
		return nil
	}
	return nil
}

func resolvedLadybugS3Credentials(cfg config.S3Config) ladybug.S3Credentials {
	endpoint := strings.TrimRight(strings.TrimPrefix(strings.TrimPrefix(cfg.Endpoint, "https://"), "http://"), "/")
	creds := ladybug.S3Credentials{
		AccessKeyID:     cfg.AccessKeyID,
		SecretAccessKey: cfg.SecretAccessKey,
		Region:          cfg.Region,
		// The scheme is stripped and re-decided by DisableSSL — see S3Credentials.Endpoint.
		Endpoint: endpoint,
		// An explicit endpoint is an S3-compatible server, and those need the bucket in the path.
		PathStyle:  cfg.Endpoint != "",
		DisableSSL: strings.HasPrefix(cfg.Endpoint, "http://"),
	}
	if !cfg.HasStaticCredentials() {
		creds.AccessKeyID = os.Getenv("AWS_ACCESS_KEY_ID")
		creds.SecretAccessKey = os.Getenv("AWS_SECRET_ACCESS_KEY")
		creds.SessionToken = os.Getenv("AWS_SESSION_TOKEN")
	}
	return creds
}

// openOnce opens the in-memory catalog and mounts the icebug bundle. The caller must hold k.mu.
//
// The catalog is ALWAYS :memory: — there is no file-based mode. What differs per store is only
// where the bundle lives: a local filesystem directory, or an S3 URI referenced from a mount
// cache of schema.cypher + icebug.json for a Hub context. Both end in the same in-memory
// catalog built from the same DDL.
func (k *LadybugBackend) openOnce() error {
	sysCfg := lbug.DefaultSystemConfig()
	sysCfg.BufferPoolSize = boundedDBBufferPool(sysCfg.BufferPoolSize, k.cfg.ReadOnly)
	sysCfg.MaxNumThreads = boundedDBThreads(sysCfg.MaxNumThreads)
	k.bufferPool = sysCfg.BufferPoolSize

	db, err := lbug.OpenInMemoryDatabase(sysCfg)
	if err != nil {
		return fmt.Errorf("ladybug open in-memory: %w", err)
	}
	conn, err := lbug.OpenConnection(db)
	if err != nil {
		db.Close()
		return fmt.Errorf("ladybug connection: %w", err)
	}

	k.db = db
	k.conn = conn

	k.prepareRemoteAccessLocked()
	// Mount icebug bundle from schema.cypher (filesystem on-the-fly, :memory: catalog).
	if err := k.mountLocalIcebugLocked(); err != nil {
		// No bundle yet on first sync is not an error – caller will build it.
		k.log().Debug("icebug mount skipped", "dir", k.cfg.IcebugDir, "error", err)
	}
	// Load canonical manifest after mount so traversal planner sees it.
	if err := k.loadCanonicalManifestLocked(); err != nil {
		return err
	}
	return nil
}

// mountLocalIcebugLocked executes schema.cypher from IcebugDir when present.
// It is idempotent and safe for :memory: catalogs rebuilt per connection.
func (k *LadybugBackend) mountLocalIcebugLocked() error {
	dir := k.cfg.IcebugDir
	if dir == "" {
		return nil
	}
	candidates := []string{
		filepath.Join(dir, "schema.cypher"),
		filepath.Join(dir, "graph.icebug", "schema.cypher"),
	}
	var schemaPath string
	for _, p := range candidates {
		if _, err := os.Stat(p); err == nil {
			schemaPath = p
			break
		}
	}
	if schemaPath == "" {
		return nil
	}
	raw, err := os.ReadFile(schemaPath)
	if err != nil {
		return err
	}
	stmts := splitCypherStatements(string(raw))
	for i, s := range stmts {
		if err := k.execQueryLocked(s); err != nil {
			return fmt.Errorf("mount schema %s stmt %d: %w", schemaPath, i+1, err)
		}
	}
	return nil
}

func (k *LadybugBackend) initSchema() error {
	// File text lives in the search index, not on the node — the only copy that
	// is actually queryable.
	ddl := []string{
		"CREATE NODE TABLE IF NOT EXISTS File(path STRING, name STRING, relative_path STRING, is_dependency BOOLEAN, lang STRING, cluster STRING, PRIMARY KEY (path))",
		"CREATE NODE TABLE IF NOT EXISTS Directory(path STRING, name STRING, cluster STRING, PRIMARY KEY (path))",
	}

	for _, q := range ddl {
		res, err := k.conn.Query(q)
		if err != nil {
			if !strings.Contains(strings.ToLower(err.Error()), "already exists") {
				return fmt.Errorf("schema %q: %w", q[:min(60, len(q))], err)
			}
		} else {
			res.Close()
		}
	}
	return nil
}

type SchemaInfo struct {
	Labels                  []string
	ContainsPairs           [][2]string
	CallerLabels            []string
	CalleeLabels            []string
	DMLTypes                []string
	DMLTargetLabels         []string
	DMLSourceLabels         []string
	ParamOwnerLabels        []string
	FieldOwnerLabels        []string
	FieldAccessSourceLabels []string
	InheritLabels           []string
	DecoratorOwnerLabels    []string
	AnnotationKinds         []string
	HasFields               bool
	HasParams               bool
	HasInherits             bool
	HasDecorators           bool
}

// callRelPairs decides which FROM→TO pairs the CALLS group declares.
//
// Extracted so the rule is testable: it is the only place that knows a FILE can call
// but is never called. A file became a caller when a call with no enclosing entity
// stopped being dropped — a top-level `init()` in a module, a bare statement in a
// script — and without the exclusion every caller label would also declare a dead
// `X → File` pair.
//
// Callees are written into Function, real or stub, so that end is always needed even
// when no caller happens to be one. calleeLabels adds the tables a resolved call can
// land in besides Function — a Method, once a call is joined to the method that
// declares it instead of to a stub of its name.
func callRelPairs(callerLabels, calleeLabels []string, nodeTables map[string]bool) [][2]string {
	targets := make([]string, 0, len(callerLabels)+len(calleeLabels)+1)
	for _, l := range callerLabels {
		if l != LabelFile {
			targets = append(targets, l)
		}
	}
	targets = append(targets, LabelFunction)
	targets = append(targets, calleeLabels...)

	seen := make(map[string]bool)
	var pairs [][2]string
	for _, from := range callerLabels {
		if !nodeTables[from] {
			continue
		}
		for _, to := range targets {
			key := from + "→" + to
			if seen[key] || !nodeTables[to] {
				continue
			}
			pairs = append(pairs, [2]string{from, to})
			seen[key] = true
		}
	}
	return pairs
}

func (k *LadybugBackend) initSchemaForLabels(info SchemaInfo) error {

	if err := k.initSchema(); err != nil {
		return err
	}

	var ddl []string

	labelSet := make(map[string]bool, len(info.Labels))
	for _, l := range info.Labels {
		labelSet[l] = true
	}
	for _, label := range info.Labels {
		safeLabel := "`" + label + "`"
		var q string
		if label == "Module" {

			q = fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS %s(uid STRING, name STRING, lang STRING, full_import_name STRING, path STRING, line_number INT64, end_line INT64, docstring STRING, cyclomatic_complexity INT64, context STRING, context_type STRING, is_dependency BOOLEAN, is_exported BOOLEAN, is_stub BOOLEAN, cluster STRING, PRIMARY KEY (uid))", safeLabel)
		} else {

			q = fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS %s(uid STRING, name STRING, path STRING, line_number INT64, end_line INT64, docstring STRING, lang STRING, cyclomatic_complexity INT64, context STRING, context_type STRING, class_context STRING, is_dependency BOOLEAN, is_exported BOOLEAN, value STRING, is_stub BOOLEAN, cluster STRING, PRIMARY KEY (uid))", safeLabel)
		}
		ddl = append(ddl, q)
	}

	// nodeTables is every label that will have a node table once this DDL runs.
	// A rel table group naming anything else is not a partial failure: LadybugDB
	// rejects the whole statement, initSchemaForLabels returns, and the rebuild
	// aborts before swapping the database in — which after --reset leaves no
	// database at all.
	nodeTables := make(map[string]bool, len(labelSet)+4)
	for l := range labelSet {
		nodeTables[l] = true
	}
	nodeTables[LabelFile] = true
	nodeTables[LabelDirectory] = true

	for _, extra := range []string{"Parameter", "Field"} {
		nodeTables[extra] = true
		if !labelSet[extra] {
			safeLabel := "`" + extra + "`"
			q := fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS %s(uid STRING, name STRING, lang STRING, is_stub BOOLEAN, PRIMARY KEY (uid))", safeLabel)
			ddl = append(ddl, q)
		}
	}

	if labelSet["Module"] {
		ddl = append(ddl, "CREATE REL TABLE IF NOT EXISTS IMPORTS(FROM File TO Module, alias STRING, full_import_name STRING, imported_name STRING, line_number INT64, source_file STRING)")
	}

	var contains []string
	contains = append(contains, "FROM Directory TO Directory", "FROM Directory TO File")
	for _, label := range info.Labels {
		contains = append(contains, fmt.Sprintf("FROM File TO `%s`", label))
	}
	for _, pair := range info.ContainsPairs {
		if !nodeTables[pair[0]] || !nodeTables[pair[1]] {
			continue
		}
		contains = append(contains, fmt.Sprintf("FROM `%s` TO `%s`", pair[0], pair[1]))
	}
	if len(contains) > 0 {
		ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS CONTAINS("+strings.Join(contains, ", ")+")")
	}

	if len(info.CallerLabels) > 0 {
		var callRels []string
		for _, pair := range callRelPairs(info.CallerLabels, info.CalleeLabels, nodeTables) {
			callRels = append(callRels, fmt.Sprintf("FROM `%s` TO `%s`", pair[0], pair[1]))
		}
		if len(callRels) > 0 {
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS CALLS("+strings.Join(callRels, ", ")+", source_file STRING, line_number INT64, full_call_name STRING, receiver_type STRING)")
		}
	}

	if info.HasParams {
		var paramRels []string
		for _, owner := range info.ParamOwnerLabels {
			if nodeTables[owner] {
				paramRels = append(paramRels, fmt.Sprintf("FROM `%s` TO `Parameter`", owner))
			}
		}
		if len(paramRels) > 0 {
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS HAS_PARAMETER("+strings.Join(paramRels, ", ")+", source_file STRING, line_number INT64)")
		}
	}

	if info.HasFields {
		var fieldOwners []string
		for _, l := range info.FieldOwnerLabels {
			if nodeTables[l] {
				fieldOwners = append(fieldOwners, fmt.Sprintf("FROM `%s` TO `Field`", l))
			}
		}
		if len(fieldOwners) > 0 {
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS HAS_FIELD("+strings.Join(fieldOwners, ", ")+", source_file STRING, line_number INT64)")
		}
		// Field access starts at whatever actually accessed the field — a method as
		// readily as a function. Function stays in the list unconditionally because
		// the stub end has to exist even when nothing in this corpus reads a field.
		accessSources := append([]string{LabelFunction}, info.FieldAccessSourceLabels...)
		accessSources = append(accessSources, info.CallerLabels...)
		seenAccess := make(map[string]bool)
		var accessRels []string
		for _, cl := range accessSources {
			if !nodeTables[cl] || seenAccess[cl] {
				continue
			}
			seenAccess[cl] = true
			accessRels = append(accessRels, fmt.Sprintf("FROM `%s` TO `Field`", cl))
		}
		if len(accessRels) > 0 {
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS READS_FIELD("+strings.Join(accessRels, ", ")+", source_file STRING, line_number INT64)")
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS WRITES_FIELD("+strings.Join(accessRels, ", ")+", source_file STRING, line_number INT64)")
		}
	}

	if info.HasInherits {
		var typeish []string
		for _, l := range info.InheritLabels {
			if nodeTables[l] {
				typeish = append(typeish, l)
			}
		}
		if len(typeish) > 0 {
			var inhRels []string
			for _, from := range typeish {
				for _, to := range typeish {
					inhRels = append(inhRels, fmt.Sprintf("FROM `%s` TO `%s`", from, to))
				}
			}
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS INHERITS("+strings.Join(inhRels, ", ")+", source_file STRING, line_number INT64)")
			ddl = append(ddl, "CREATE REL TABLE GROUP IF NOT EXISTS IMPLEMENTS("+strings.Join(inhRels, ", ")+", source_file STRING, line_number INT64)")
		}
	}

	if info.HasDecorators {
		for _, kind := range info.AnnotationKinds {
			ddl = append(ddl, fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS `%s`(uid STRING, name STRING, lang STRING, is_stub BOOLEAN, PRIMARY KEY (uid))", kind))
			nodeTables[kind] = true
		}

		for _, kind := range info.AnnotationKinds {
			edgeName := "HAS_" + strings.ToUpper(kind)
			var rels []string
			for _, l := range info.DecoratorOwnerLabels {
				if nodeTables[l] {
					rels = append(rels, fmt.Sprintf("FROM `%s` TO `%s`", l, kind))
				}
			}
			if len(rels) > 0 {
				ddl = append(ddl, fmt.Sprintf("CREATE REL TABLE GROUP IF NOT EXISTS %s(%s, source_file STRING, line_number INT64)", edgeName, strings.Join(rels, ", ")))
			}
		}
	}

	if len(info.DMLTypes) > 0 {
		// The targets are the labels the references actually resolve to, not a
		// fixed list of object kinds: the fixed list dropped every ALTERS on an
		// index and, because it required a Table label to be present, produced no
		// DML edges at all for a corpus whose tables were never extracted.
		var dmlTargets []string
		for _, l := range info.DMLTargetLabels {
			if nodeTables[l] {
				dmlTargets = append(dmlTargets, l)
			}
		}

		dmlSources := info.DMLSourceLabels
		if len(dmlSources) == 0 {

			dmlSources = info.CallerLabels
		}
		if len(dmlTargets) > 0 {
			for _, dmlType := range info.DMLTypes {
				var dmlRels []string
				for _, s := range dmlSources {
					if nodeTables[s] {
						for _, t := range dmlTargets {
							dmlRels = append(dmlRels, fmt.Sprintf("FROM `%s` TO `%s`", s, t))
						}
					}
				}
				for _, t := range dmlTargets {
					dmlRels = append(dmlRels, fmt.Sprintf("FROM File TO `%s`", t))
				}
				if len(dmlRels) > 0 {
					ddl = append(ddl, fmt.Sprintf("CREATE REL TABLE GROUP IF NOT EXISTS %s(%s, source_file STRING, line_number INT64)", dmlType, strings.Join(dmlRels, ", ")))
				}
			}
		}
	}

	for _, q := range ddl {
		res, err := k.conn.Query(q)
		if err != nil {
			if !strings.Contains(strings.ToLower(err.Error()), "already exists") {
				return fmt.Errorf("schema %q: %w", q[:min(60, len(q))], err)
			}
		} else {
			res.Close()
		}
	}
	return nil
}

// ensureConnected connects if needed. The caller must hold k.mu; use
// ensureConnectedLocked otherwise.
func (k *LadybugBackend) ensureConnected() error {

	if k.canonical == nil {
		if err := k.loadCanonicalManifestLocked(); err != nil {
			return fmt.Errorf("icebug: canonical manifest: %w", err)
		}
	}
	if k.conn != nil {
		return nil
	}
	if err := k.connectLocked(); err != nil {
		k.log().Error("ladybug: connection unavailable", "icebug_dir", k.cfg.IcebugDir, "error", err)
		return err
	}
	return nil
}

func (k *LadybugBackend) runQuery(cypher string, params map[string]any) (*lbug.QueryResult, error) {
	translated, tParams := translateLadybug(cypher, params)

	if len(tParams) == 0 {
		return k.conn.Query(translated)
	}

	stmt, ok := k.stmtCache[translated]
	if !ok {
		var err error
		stmt, err = k.conn.Prepare(translated)
		if err != nil {
			return nil, fmt.Errorf("ladybug prepare: %w", err)
		}
		k.stmtCache[translated] = stmt
	}

	return k.conn.Execute(stmt, tParams)
}

func (k *LadybugBackend) Query(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error) {
	k.mu.Lock()
	defer k.mu.Unlock()

	if err := k.ensureConnected(); err != nil {
		return nil, err
	}
	if k.canonical != nil {
		cypher = sanitizeCanonicalUIDEquality(cypher)
		cypher = sanitizeCanonicalPKEquality(k.canonical, cypher)
		if res, handled, err := k.tryCanonicalBoundedTraversal(ctx, cypher, params); handled {
			if err != nil {
				return nil, fmt.Errorf("ladybug query: %w", err)
			}
			return res, nil
		}
		// A query the bounded planner did not recognize falls through to the engine
		// WHEN it can: a node-only pattern (or one naming a PHYSICAL member table)
		// runs on the mounted tables exactly as written — but equality against a
		// PRIMARY KEY is still rewritten to IN, because the icebug engine answers
		// `path = 'x'` with zero rows even when the row exists. What cannot run is
		// a traversal that names a LOGICAL relationship type — CALLS, CONTAINS —
		// whose real member tables the engine has no way to fuse into one type.
		// That is the same fail-closed contract the Hub had; the planner is the
		// only route.
		if namesLogicalRel(k.canonical, cypher) {
			var members []string
			for _, g := range k.canonical.RelGroups {
				members = append(members, g.Type)
			}
			return nil, fmt.Errorf("canonical catalog: only bounded reachability over %v is planned "+
				"(RETURN DISTINCT <endpoint> | count([DISTINCT] endpoint.uid)); this multi-hop form is not supported remotely", members)
		}
	}

	res, err := k.runQuery(cypher, params)
	if err != nil {
		if strings.Contains(strings.ToLower(err.Error()), "already exists") {
			return &QueryResult{}, nil
		}
		if hint := k.explainBinderErrorLocked(err); hint != "" {
			return nil, fmt.Errorf("ladybug query: %w — %s", err, hint)
		}
		return nil, fmt.Errorf("ladybug query: %w", err)
	}
	defer res.Close()
	return ladybugResultToQueryResult(res)
}

// explainBinderErrorLocked turns an engine error into the answer to the question it
// raises, or "" when it has nothing to add. The caller must hold k.mu.
func (k *LadybugBackend) explainBinderErrorLocked(err error) string {
	if m := reMissingTable.FindStringSubmatch(err.Error()); m != nil {
		return missingTableMessage(err, k.tableNamesLocked())
	}
	if m := reMissingProperty.FindStringSubmatch(err.Error()); m != nil {
		return k.missingPropertyMessageLocked(m[1])
	}
	return ""
}

// missingPropertyMessageLocked explains "Cannot find property line_number for e".
//
// The engine binds a property on an unlabelled match when ANY candidate table has it,
// so this error usually means a name that exists nowhere — `line` for `line_number`,
// `type` for label(n). But it has a second cause that reads identically and sends the
// reader off to fix a query that was already correct: during a rebuild the graph holds
// a partial schema, and a property every entity normally carries is momentarily on no
// table at all. That is what happened to `MATCH (f:File)-[:CONTAINS]->(e) RETURN
// e.line_number` mid-reindex — a correct query, answered as though it were wrong.
//
// So the two are distinguished by whether anything in this graph has the property.
// The caller must hold k.mu.
func (k *LadybugBackend) missingPropertyMessageLocked(prop string) string {
	holders := k.tablesWithPropertyLocked(prop)

	if len(holders) == 0 {
		tables := k.tableNamesLocked()
		return fmt.Sprintf("no label in this graph has a property %q. Either the name is wrong "+
			"— `line` is `line_number`, and the node type is `label(n)`, not a property — or the "+
			"graph is mid-rebuild and its schema is still partial, which is what these %d tables "+
			"look like: %s", prop, len(tables), strings.Join(tables, ", "))
	}

	const shownMax = 20
	shown, extra := holders, ""
	if len(shown) > shownMax {
		shown, extra = shown[:shownMax], fmt.Sprintf(", and %d more", len(holders)-shownMax)
	}

	return fmt.Sprintf("%q exists, but not on every label this pattern can match — it is on %s%s. "+
		"Pin the label in the pattern (`MATCH (n:Function)`), because `WHERE label(n) IN [...]` "+
		"filters after binding and does not help here", prop, strings.Join(shown, ", "), extra)
}

// tablesWithPropertyLocked lists the tables carrying a property. The caller must hold
// k.mu, which is why it cannot go through Query.
func (k *LadybugBackend) tablesWithPropertyLocked(prop string) []string {
	var holders []string
	for _, t := range k.tableNamesLocked() {
		res, err := k.runQuery(fmt.Sprintf("CALL table_info('%s') RETURN *", t), nil)
		if err != nil {
			continue
		}
		qr, err := ladybugResultToQueryResult(res)
		res.Close()
		if err != nil {
			continue
		}
		for _, rec := range qr.Records {
			if name, ok := rec["name"].(string); ok && name == prop {
				holders = append(holders, t)
				break
			}
		}
	}
	return holders
}

// missingTableMessage explains "Table Import does not exist", and returns "" for any
// other error.
//
// A label that no indexed file produced has no table, so matching it is a hard error
// here where Neo4j would answer zero rows. The raw message reads like the graph is
// broken, when what it means is narrower and more useful: that name is not in THIS
// project's graph. Which labels are present is the answer to the next question, so it
// comes along.
//
// It stays an error on purpose. Answering an empty result would be friendlier and
// worse: a typo would become indistinguishable from an honest absence, and the query
// would look answered.
func missingTableMessage(err error, present []string) string {
	m := reMissingTable.FindStringSubmatch(err.Error())
	if m == nil {
		return ""
	}
	name := m[1]

	if len(present) == 0 {
		return fmt.Sprintf("%q is not a label or relationship type in this graph, "+
			"and the graph reports no tables at all — it is empty or mid-rebuild", name)
	}

	const shownMax = 40
	shown, extra := present, ""
	if len(shown) > shownMax {
		shown, extra = shown[:shownMax], fmt.Sprintf(", and %d more", len(present)-shownMax)
	}

	return fmt.Sprintf("%q is not a label or relationship type in this project's graph: "+
		"nothing indexed here produced it, so it has no table. Present: %s%s",
		name, strings.Join(shown, ", "), extra)
}

// tableNamesLocked lists the node and relationship tables. The caller must hold k.mu,
// which is why it cannot go through Query.
func (k *LadybugBackend) tableNamesLocked() []string {
	res, err := k.runQuery("CALL show_tables() RETURN *", nil)
	if err != nil {
		return nil
	}
	defer res.Close()

	qr, err := ladybugResultToQueryResult(res)
	if err != nil {
		return nil
	}

	var names []string
	for _, rec := range qr.Records {
		if name, ok := rec["name"].(string); ok {
			names = append(names, name)
		}
	}
	sort.Strings(names)
	return names
}

func (k *LadybugBackend) Execute(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error) {
	return k.Query(ctx, cypher, params)
}

func (k *LadybugBackend) ExecuteBatch(ctx context.Context, queries []BatchQuery) error {
	k.mu.Lock()
	defer k.mu.Unlock()

	if err := k.ensureConnected(); err != nil {
		return err
	}

	var nodeQueries, edgeQueries []BatchQuery
	for _, q := range queries {
		upper := strings.ToUpper(q.Cypher)
		hasMatch := strings.Contains(upper, "MATCH")
		hasMerge := strings.Contains(upper, "MERGE")
		hasDelete := strings.Contains(upper, "DELETE")

		if hasDelete || (hasMerge && !hasMatch) {
			nodeQueries = append(nodeQueries, q)
		} else {
			edgeQueries = append(edgeQueries, q)
		}
	}

	var allErrs []string

	if errs := k.runBatchPhase(nodeQueries); len(errs) > 0 {
		allErrs = append(allErrs, errs...)
	}

	if errs := k.runBatchPhase(edgeQueries); len(errs) > 0 {
		allErrs = append(allErrs, errs...)
	}

	if len(allErrs) > 0 {
		return fmt.Errorf("ladybug batch (%d errors): %s", len(allErrs), allErrs[0])
	}
	return nil
}

func (k *LadybugBackend) runBatchPhase(queries []BatchQuery) []string {
	if len(queries) == 0 {
		return nil
	}

	tx, err := k.conn.Query("BEGIN TRANSACTION")
	if err == nil {
		tx.Close()
	}

	var errs []string

	for _, q := range queries {

		res, err := k.runQuery(q.Cypher, q.Params)
		if err != nil {
			errLower := strings.ToLower(err.Error())

			if strings.Contains(errLower, "already exists") {
				continue
			}

			upper := strings.ToUpper(q.Cypher)
			isCrashBug := strings.Contains(errLower, "unordered_map") ||
				strings.Contains(errLower, "runtime error") ||
				strings.Contains(errLower, "status 1")
			hasUnwind := strings.Contains(upper, "UNWIND")

			if isCrashBug && hasUnwind {
				rowErrs := k.executePerRow(q)
				errs = append(errs, rowErrs...)
			} else {

				qShort := q.Cypher
				if len(qShort) > 80 {
					qShort = qShort[:80] + "…"
				}
				k.log().Error("batch query failed", "query", qShort, "error", err)
				errs = append(errs, err.Error())
			}
		} else {
			res.Close()
		}
	}

	if txRes, err := k.conn.Query("COMMIT"); err == nil {
		txRes.Close()
	}

	return errs
}

func (k *LadybugBackend) executePerRow(q BatchQuery) []string {
	data, ok := q.Params["data"]
	if !ok {
		return nil
	}
	rows, ok := data.([]map[string]any)
	if !ok {
		return nil
	}
	if len(rows) == 0 {
		return nil
	}

	cypher := q.Cypher
	idx := strings.Index(strings.ToUpper(cypher), "MATCH")
	if idx < 0 {
		idx = strings.Index(strings.ToUpper(cypher), "MERGE")
	}
	if idx < 0 {
		return []string{"executePerRow: cannot parse query: " + cypher}
	}
	body := cypher[idx:]

	body = strings.ReplaceAll(body, "row.", "$")

	translated, _ := translateLadybug(body, rows[0])

	usedKeys := make([]string, 0, len(rows[0]))
	for k := range rows[0] {
		if strings.Contains(translated, "$"+k) {
			usedKeys = append(usedKeys, k)
		}
	}

	stmt, ok := k.stmtCache[translated]
	if !ok {
		var err error
		stmt, err = k.conn.Prepare(translated)
		if err != nil {
			return []string{fmt.Sprintf("executePerRow prepare: %v", err)}
		}
		k.stmtCache[translated] = stmt
	}

	var errs []string
	for _, row := range rows {

		p := make(map[string]any, len(usedKeys))
		for _, k := range usedKeys {
			p[k] = row[k]
		}
		res, err := k.conn.Execute(stmt, p)
		if err != nil {
			errLower := strings.ToLower(err.Error())
			if strings.Contains(errLower, "already exists") {
				continue
			}
			errs = append(errs, err.Error())
		} else {
			res.Close()
		}
	}
	return errs
}

func (k *LadybugBackend) Ping(ctx context.Context) error {
	k.mu.Lock()
	defer k.mu.Unlock()
	if err := k.ensureConnected(); err != nil {
		return err
	}
	res, err := k.conn.Query("RETURN 1")
	if err != nil {
		return err
	}
	res.Close()
	return nil
}

func (k *LadybugBackend) BackendType() string { return "ladybug" }

// StoreDir returns the global store directory this backend serves. The backend
// connects lazily, so this is the only way for a caller to check where a
// configured backend actually points before it touches the disk.
func (k *LadybugBackend) StoreDir() string { return k.cfg.StoreDir }

func (k *LadybugBackend) Shutdown() error {
	k.mu.Lock()
	defer k.mu.Unlock()

	if k.conn == nil || k.db == nil {
		return nil
	}

	res, err := k.conn.Query("CHECKPOINT")
	if err != nil {
		k.log().Warn("ladybug shutdown: CHECKPOINT failed", "error", err)
		return fmt.Errorf("ladybug shutdown: CHECKPOINT: %w", err)
	}
	res.Close()
	return nil
}

func (k *LadybugBackend) Close() error {
	k.mu.Lock()
	defer k.mu.Unlock()
	for _, stmt := range k.stmtCache {
		stmt.Close()
	}
	k.stmtCache = make(map[string]*lbug.PreparedStatement)

	if k.conn != nil {
		k.conn.Close()
		k.conn = nil
	}
	if k.db != nil {
		k.db.Close()
		k.db = nil
	}

	return nil
}

func (k *LadybugBackend) execQuery(q string) error {
	k.mu.Lock()
	defer k.mu.Unlock()
	if err := k.ensureConnected(); err != nil {
		return err
	}
	return k.execQueryLocked(q)
}

// execQueryLocked runs a statement on an already-open connection. The caller must hold k.mu AND
// have connected — it exists for the setup that runs INSIDE the open, where taking the lock again
// would deadlock and ensureConnected would recurse.
func (k *LadybugBackend) execQueryLocked(q string) error {
	res, err := k.conn.Query(q)
	if err != nil {
		return err
	}
	res.Close()
	return nil
}

func ladybugResultToQueryResult(res *lbug.QueryResult) (*QueryResult, error) {
	qr := &QueryResult{}
	cols := res.GetColumnNames()

	for res.HasNext() {
		tuple, err := res.Next()
		if err != nil {
			break
		}
		record := make(QueryRecord, len(cols))
		for i, col := range cols {
			func() {
				defer func() {
					if r := recover(); r != nil {

						record[col] = nil
					}
				}()
				val, err := tuple.GetValue(uint64(i))
				if err == nil {
					record[col] = normalizeLadybugValue(val)
				}
			}()
		}
		qr.Records = append(qr.Records, record)
	}
	return qr, nil
}

func normalizeLadybugValue(val any) any {
	switch v := val.(type) {
	case lbug.Node:
		return map[string]any{
			"ID":         map[string]any{"TableID": v.ID.TableID, "Offset": v.ID.Offset},
			"Label":      v.Label,
			"Properties": v.Properties,
		}
	case lbug.Relationship:
		return map[string]any{
			"ID":            map[string]any{"TableID": v.ID.TableID, "Offset": v.ID.Offset},
			"SourceID":      map[string]any{"TableID": v.SourceID.TableID, "Offset": v.SourceID.Offset},
			"DestinationID": map[string]any{"TableID": v.DestinationID.TableID, "Offset": v.DestinationID.Offset},
			"Label":         v.Label,
			"Properties":    v.Properties,
		}
	default:
		return val
	}
}

func translateLadybug(cypher string, params map[string]any) (string, map[string]any) {
	q := cypher

	// LadybugDB has no secondary indexes and no constraints, so those statements are
	// answered without running anything. Anchoring at the start is what makes this a
	// test on the statement: `strings.Contains` over the whole query also fired on a
	// literal, so searching the codebase for the text 'CREATE INDEX' returned 1
	// instead of the comments that contain it.
	if reDDLNoop.MatchString(q) {
		return "RETURN 1", map[string]any{}
	}

	q = mapOutsideStrings(q, func(seg string) string {
		seg = reOnCreateSet.ReplaceAllString(seg, "SET")
		seg = reOnMatchSet.ReplaceAllString(seg, "SET")

		// Neo4j's labels(x)[0] is Ladybug's label(x), for any variable — this used
		// to be a literal replace of `labels(n)[0]`, so it worked only when the
		// variable happened to be named n.
		seg = reLabelsIndex.ReplaceAllString(seg, "label($1)")

		seg = reTypeParen.ReplaceAllString(seg, "label(")

		// The label predicate is rewritten BEFORE any label is escaped. Escaping
		// first puts a backtick exactly where this regex expects the label name, so
		// the rewrite never fires — which is how `WHERE n:Function` came to fail
		// while `WHERE n:Method` worked. Method was missing from the old escape
		// list, so it was the one form left in a shape this could still match: the
		// rewrite only worked for labels the escaper did not know about.
		return reLabelPredicate.ReplaceAllString(seg, "${1}label(${2}) = '${3}'")
	})

	// A second pass, because the rewrite above emits string literals of its own and
	// escaping must see them as literals.
	q = mapOutsideStrings(q, func(seg string) string {
		seg = rePatternLabel.ReplaceAllString(seg, "$1`$2`")

		// The alternatives after the first, now that the first one is escaped and no
		// longer looks like one of them.
		seg = reRelBracket.ReplaceAllStringFunc(seg, func(bracket string) string {
			if !reRelTypeList.MatchString(bracket) {
				return bracket
			}
			return reRelAltType.ReplaceAllString(bracket, "$1`$2`")
		})

		return strings.ReplaceAll(seg, "``", "`")
	})

	usedParams := map[string]bool{}
	for _, match := range reParamUsage.FindAllStringSubmatch(q, -1) {
		usedParams[match[1]] = true
	}
	cleanParams := make(map[string]any, len(params))
	for k, v := range params {
		if usedParams[k] {
			cleanParams[k] = v
		}
	}

	return q, cleanParams
}
