package ast

import (
	"context"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"sync"

	lbug "github.com/LadybugDB/go-ladybug"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/slogutil"
)

var (
	escapeRegexes   []*regexp.Regexp
	escapeRepls     []string
	escapeRegexOnce sync.Once

	reOnCreateSet = regexp.MustCompile(`(?i)\bON\s+CREATE\s+SET\b`)
	reOnMatchSet  = regexp.MustCompile(`(?i)\bON\s+MATCH\s+SET\b`)
	reTypeParen   = regexp.MustCompile(`\btype\(`)
	reLabelFilter = regexp.MustCompile(`(?i)(WHERE\s+|AND\s+|OR\s+)(\w+):([a-zA-Z0-9_]+)`)
	reParamUsage  = regexp.MustCompile(`\$(\w+)`)
)

func initEscapeRegexes() {

	allLabels := map[string]bool{

		"File": true, "Directory": true, "Module": true, "Function": true,
		"Class": true, "Variable": true, "Trait": true, "Interface": true,
		"Macro": true, "Struct": true, "Enum": true, "Union": true,
		"Annotation": true, "Record": true, "Property": true, "Parameter": true,
		"Field": true, "Table": true, "View": true, "Procedure": true,
		"Package": true, "Trigger": true, "Index": true, "Sequence": true,
		"Type": true, "Synonym": true, "Constant": true, "Cursor": true,
		"Exception": true, "Namespace": true, "Export": true, "Delegate": true,
		"Comment": true, "Constraint": true, "MaterializedView": true,
		"DatabaseLink": true, "Column": true,

		"CONTAINS": true, "IMPORTS": true, "CALLS": true, "HAS_PARAMETER": true,
		"INHERITS": true, "IMPLEMENTS": true, "HAS_FIELD": true,
		"READS_FIELD": true, "WRITES_FIELD": true,
		"SELECTS": true, "INSERTS": true, "UPDATES": true, "DELETES": true,
		"CREATES": true, "ALTERS": true, "DROPS": true, "REFERENCES": true,
		"INCLUDES": true,
	}

	for kw := range allLabels {
		escapeRegexes = append(escapeRegexes, regexp.MustCompile(`:`+kw+`\b`))
		escapeRepls = append(escapeRepls, ":`"+kw+"`")
	}
}

type LadybugConfig struct {
	DBPath string

	ReadOnly bool
}

func DefaultLadybugConfig() LadybugConfig {
	return LadybugConfig{
		DBPath: envOr("LADYBUGDB_PATH",
			filepath.Join(brand.DotDir(), "ast", "project", "ladybugdb")),
	}
}

func LadybugConfigForContext(name string) LadybugConfig {
	if name == "" || name == "__project__" {
		return DefaultLadybugConfig()
	}
	return LadybugConfig{
		DBPath: ContextDBPath(name),
	}
}

type LadybugBackend struct {
	cfg    LadybugConfig
	db     *lbug.Database
	conn   *lbug.Connection
	mu     sync.Mutex
	once   sync.Once
	Logger *slog.Logger

	stmtCache  map[string]*lbug.PreparedStatement
	connectErr error
}

func (k *LadybugBackend) log() *slog.Logger { return slogutil.Resolve(k.Logger) }

func NewLadybugDB(cfg LadybugConfig) *LadybugBackend {
	return &LadybugBackend{cfg: cfg, stmtCache: make(map[string]*lbug.PreparedStatement)}
}

func NewLadybugDBReadOnly(cfg LadybugConfig) *LadybugBackend {
	cfg.ReadOnly = true
	return &LadybugBackend{cfg: cfg, stmtCache: make(map[string]*lbug.PreparedStatement)}
}

func (k *LadybugBackend) connect() error {
	k.once.Do(func() {

		CleanupInterruptedSwap(k.cfg.DBPath)

		if err := os.MkdirAll(filepath.Dir(k.cfg.DBPath), 0o755); err != nil {
			k.connectErr = fmt.Errorf("ladybug: create dir: %w", err)
			k.log().Error("ladybug: failed to create directory", "path", k.cfg.DBPath, "error", err)
			return
		}

		sysCfg := lbug.DefaultSystemConfig()
		// Bound the buffer pool (default ~80% RAM) and native thread pool
		// (default NumCPU) so the indexer stays machine-friendly — especially
		// during incremental rebuilds, when a working DB is open alongside the
		// production DB. Overridable via GRAPHIT_DB_BUFFER_MB / GRAPHIT_DB_THREADS.
		sysCfg.BufferPoolSize = boundedDBBufferPool(sysCfg.BufferPoolSize)
		sysCfg.MaxNumThreads = boundedDBThreads(sysCfg.MaxNumThreads)
		if k.cfg.ReadOnly {
			sysCfg.ReadOnly = true
		}

		// Share one *lbug.Database per path across backends in this process so a
		// reader gets snapshot isolation against an in-process writer (see
		// ladybug_registry.go).
		db, err := acquireDatabase(k.cfg.DBPath, sysCfg)
		if err != nil {
			k.connectErr = fmt.Errorf("ladybug open: %w", err)
			k.log().Error("ladybug: failed to open database", "path", k.cfg.DBPath, "readonly", k.cfg.ReadOnly, "error", err)
			return
		}
		conn, err := lbug.OpenConnection(db)
		if err != nil {
			k.connectErr = fmt.Errorf("ladybug connection: %w", err)
			k.log().Error("ladybug: failed to open connection", "path", k.cfg.DBPath, "error", err)
			releaseDatabase(k.cfg.DBPath, db) // drop our reference if the connection failed
			return
		}
		k.db = db
		k.conn = conn
	})
	return k.connectErr
}

func (k *LadybugBackend) initSchema() error {
	ddl := []string{
		"CREATE NODE TABLE IF NOT EXISTS File(path STRING, name STRING, relative_path STRING, is_dependency BOOLEAN, lang STRING, cluster STRING, source STRING, PRIMARY KEY (path))",
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
	Labels               []string
	ContainsPairs        [][2]string
	CallerLabels         []string
	DMLTypes             []string
	DMLTargetLabels      []string
	DMLSourceLabels      []string
	ParamOwnerLabels     []string
	FieldOwnerLabels     []string
	InheritLabels        []string
	DecoratorOwnerLabels []string
	AnnotationKinds      []string
	HasFields            bool
	HasParams            bool
	HasInherits          bool
	HasDecorators        bool
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

			q = fmt.Sprintf("CREATE NODE TABLE IF NOT EXISTS %s(uid STRING, name STRING, path STRING, line_number INT64, end_line INT64, docstring STRING, lang STRING, cyclomatic_complexity INT64, context STRING, context_type STRING, class_context STRING, is_dependency BOOLEAN, is_exported BOOLEAN, value STRING, is_stub BOOLEAN, entry_point_score INT64, cluster STRING, PRIMARY KEY (uid))", safeLabel)
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
		// Callees are written into Function — real or stub — so the group needs
		// that end even when no caller happens to be one.
		callTargets := append([]string{}, info.CallerLabels...)
		callTargets = append(callTargets, LabelFunction)
		seen := make(map[string]bool)
		var callRels []string
		for _, from := range info.CallerLabels {
			if !nodeTables[from] {
				continue
			}
			for _, to := range callTargets {
				key := from + "→" + to
				if seen[key] || !nodeTables[to] {
					continue
				}
				callRels = append(callRels, fmt.Sprintf("FROM `%s` TO `%s`", from, to))
				seen[key] = true
			}
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
		// Field access is written from Function, so that end is required and the
		// caller labels are the extra ones.
		accessSources := append([]string{LabelFunction}, info.CallerLabels...)
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

// ensureConnectedLocked connects while taking the backend mutex. Use from
// callers that are not already holding it (the in-place incremental path).
func (k *LadybugBackend) ensureConnectedLocked() error {
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.ensureConnected()
}

func (k *LadybugBackend) ensureConnected() error {
	if k.conn != nil {
		return nil
	}
	if err := k.connect(); err != nil {
		k.log().Error("ladybug: connection unavailable", "path", k.cfg.DBPath, "error", err)
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

	res, err := k.runQuery(cypher, params)
	if err != nil {
		if strings.Contains(strings.ToLower(err.Error()), "already exists") {
			return &QueryResult{}, nil
		}
		return nil, fmt.Errorf("ladybug query: %w", err)
	}
	defer res.Close()
	return ladybugResultToQueryResult(res)
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
		// The handle may be shared with other backends in this process; the
		// registry closes it only when the last reference goes away.
		releaseDatabase(k.cfg.DBPath, k.db)
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

	qUpper := strings.ToUpper(q)
	if strings.Contains(qUpper, "CREATE CONSTRAINT") || strings.Contains(qUpper, "CREATE INDEX") {
		return "RETURN 1", map[string]any{}
	}

	q = reOnCreateSet.ReplaceAllString(q, "SET")
	q = reOnMatchSet.ReplaceAllString(q, "SET")

	q = strings.ReplaceAll(q, "labels(n)[0]", "label(n)")

	q = reTypeParen.ReplaceAllString(q, "label(")

	q = strings.ReplaceAll(q, "coalesce(", "COALESCE(")

	escapeRegexOnce.Do(initEscapeRegexes)
	for i, re := range escapeRegexes {
		q = re.ReplaceAllString(q, escapeRepls[i])
	}
	q = strings.ReplaceAll(q, "``", "`")

	q = reLabelFilter.
		ReplaceAllStringFunc(q, func(match string) string {
			m := reLabelFilter.FindStringSubmatch(match)
			if m == nil {
				return match
			}
			return m[1] + "label(" + m[2] + ") = '" + m[3] + "'"
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

func (k *LadybugBackend) AtomicSwapDB(newDBPath string) error {
	currentPath := k.cfg.DBPath
	oldPath := currentPath + ".old"

	_ = os.RemoveAll(oldPath)

	if err := os.Rename(currentPath, oldPath); err != nil && !os.IsNotExist(err) {
		return fmt.Errorf("atomic swap: rename current→old: %w", err)
	}

	if err := os.Rename(newDBPath, currentPath); err != nil {
		if restoreErr := os.Rename(oldPath, currentPath); restoreErr != nil {
			return fmt.Errorf("atomic swap CRITICAL: new→current failed (%w) AND restore failed (%w)", err, restoreErr)
		}
		return fmt.Errorf("atomic swap: rename new→current: %w", err)
	}

	_ = os.RemoveAll(oldPath)
	_ = os.Remove(currentPath + ".wal")

	return nil
}

func CleanupInterruptedSwap(dbPath string) {
	_ = os.RemoveAll(dbPath + ".old")

	matches, _ := filepath.Glob(dbPath + ".*")
	for _, m := range matches {
		if m == dbPath+".wal" || strings.HasPrefix(m, dbPath+searchIndexSuffix) {
			continue
		}
		_ = os.RemoveAll(m)
	}

	_ = os.RemoveAll(dbPath + ".staging")
}
