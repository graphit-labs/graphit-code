package ast

import (
	"context"
	"fmt"
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

const canonicalImpactQuery = "MATCH (caller)-[:CALLS*1..3]->(t) WHERE " +
	"(t.name = 'runQuery' OR t.uid IN ['internal/ast/ladybug.go::runQuery']) " +
	"RETURN DISTINCT caller.uid AS uid"

func uidSetsEqual(a, b *QueryResult) bool {
	as, bs := recordStrings(a, "uid"), recordStrings(b, "uid")
	return len(as) > 0 && len(bs) > 0 && sameStrings(as, bs)
}

func buildCanonicalFixture(t *testing.T) (mounted *LadybugBackend) {
	t.Helper()
	names := []string{"a", "b", "c", "d", "e", "f"}
	var ents []cachedEntity
	var calls []cachedCall
	for _, n := range names {
		ents = append(ents, cachedEntity{Label: "Function", UID: "fn_" + n, Name: n, Path: "f.go", Line: 1, EndLine: 2})
	}
	for i := 0; i+1 < len(names); i++ {
		calls = append(calls, cachedCall{CallerUID: "fn_" + names[i], CalleeUID: "fn_" + names[i+1], SourceType: "Function", Path: "f.go", Line: 1, Lang: "go"})
	}
	entry := &parseCacheEntry{RelPath: "f.go", Language: "go", Entities: ents, Calls: calls}
	ctx := context.Background()
	_ = ctx

	bundle := filepath.Join(t.TempDir(), "bundle")
	ri := newRebuildIndex(map[string]*parseCacheEntry{"f.go": entry}, targetRulesFor(""))
	if _, err := ExportDirectFromRebuildIndex(ri, bundle, bundle); err != nil {
		t.Fatalf("canonical export: %v", err)
	}

	schemaRaw, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("schema: %v", err)
	}
	mountDir := t.TempDir()
	if err := MountIcebugGraph(ctx, mountDir, string(schemaRaw), nil); err != nil {
		t.Fatalf("mount: %v", err)
	}
	manifestRaw, err := os.ReadFile(filepath.Join(bundle, ladybug.IcebugManifestFile))
	if err != nil {
		t.Fatalf("read manifest: %v", err)
	}
	if err := os.WriteFile(filepath.Join(mountDir, ladybug.IcebugManifestFile), manifestRaw, 0o644); err != nil {
		t.Fatalf("stage manifest: %v", err)
	}

	mounted = NewLadybugDBReadOnly(LadybugConfig{StoreDir: mountDir, IcebugDir: mountDir})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	t.Cleanup(func() { _ = mounted.Close() })
	return mounted
}

func TestMountedCanonicalUnboundedTraversalMatchesNative(t *testing.T) {
	mounted := buildCanonicalFixture(t)

	publicQ := "MATCH (caller)-[:CALLS*]->(t) WHERE t.uid IN ['fn_f'] RETURN DISTINCT caller.uid AS uid"
	got, err := mounted.Query(context.Background(), publicQ, nil)
	if err != nil {
		t.Fatalf("canonical planner: %v", err)
	}
	gotUIDs := recordStrings(got, "uid")
	if len(gotUIDs) != 5 {
		t.Fatalf("uncapped traversal returned %v, want the full 5-hop chain a..e", gotUIDs)
	}
	seen := map[string]bool{}
	for _, u := range gotUIDs {
		seen[u] = true
	}
	for _, n := range []string{"a", "b", "c", "d", "e"} {
		if !seen["fn_"+n] {
			t.Fatalf("missing caller fn_%s (got %v)", n, gotUIDs)
		}
	}
}

func TestMountedCanonicalCountDistinct(t *testing.T) {
	mounted := buildCanonicalFixture(t)
	res, err := mounted.Query(context.Background(),
		"MATCH (caller)-[:CALLS*1..3]->(t) WHERE t.uid IN ['fn_f'] RETURN count(DISTINCT caller.uid)", nil)
	if err != nil {
		t.Fatalf("count distinct: %v", err)
	}
	var got int64
	for _, r := range res.Records {
		for _, v := range r {
			got = ladybug.Int64(v)
		}
	}
	if got != 3 {
		t.Fatalf("count(DISTINCT caller.uid)=%d want 3", got)
	}
}

func TestMountedCanonicalUnsupportedProjectionFailsClosed(t *testing.T) {
	mounted := buildCanonicalFixture(t)
	_, err := mounted.Query(context.Background(),
		"MATCH (caller)-[:CALLS*1..3]->(t) WHERE t.uid IN ['fn_f'] RETURN collect(caller.uid)", nil)
	if err == nil || !stringsContains(err.Error(), "must be DISTINCT") {
		t.Fatalf("want a fail-closed refusal naming its rule, got %v", err)
	}
}

func stringsContains(s, sub string) bool {
	return len(sub) == 0 || (len(s) >= len(sub) && indexOf(s, sub) >= 0)
}
func indexOf(s, sub string) int {
	for i := 0; i+len(sub) <= len(s); i++ {
		if s[i:i+len(sub)] == sub {
			return i
		}
	}
	return -1
}

func TestSanitizeCanonicalUIDEquality(t *testing.T) {
	in := "MATCH (f:Function {name:'x'}) WHERE f.uid = 'fn_1' RETURN f.name"
	want := "MATCH (f:Function {name:'x'}) WHERE f.uid IN ['fn_1'] RETURN f.name"
	if got := sanitizeCanonicalUIDEquality(in); got != want {
		t.Fatalf("got  %s\nwant %s", got, want)
	}
	lit := "MATCH (n) WHERE n.name = 'f.uid = x' RETURN n"
	if got := sanitizeCanonicalUIDEquality(lit); got != lit {
		t.Fatalf("string literal mangled:\n%s", got)
	}
}

func TestMountedCanonicalBarePatternIsExactlyOneHop(t *testing.T) {
	mounted := buildCanonicalFixture(t)
	q := "MATCH (c)-[:CALLS]->(t) WHERE t.uid IN ['fn_b'] RETURN DISTINCT c.uid AS uid"
	got, err := mounted.Query(context.Background(), q, nil)
	if err != nil {
		t.Fatalf("canonical bare pattern: %v", err)
	}
	gotUIDs := recordStrings(got, "uid")
	if len(gotUIDs) != 1 || gotUIDs[0] != "fn_a" {
		t.Fatalf("bare single-hop returned %v; want only the direct caller fn_a", gotUIDs)
	}
}

func TestMountedCanonicalRealGraphTraversalCost(t *testing.T) {
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if storePath == "" {
		t.Skip("set GRAPHIT_REAL_STORE to a populated ladybugdb")
	}
	native := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(storePath), IcebugDir: filepath.Join(filepath.Dir(storePath), "graph.icebug")})
	if err := native.connect(); err != nil {
		t.Fatalf("open native: %v", err)
	}
	defer func() { _ = native.Close() }()
	ctx := context.Background()

	want, err := native.Query(ctx, canonicalImpactQuery, nil)
	if err != nil {
		t.Fatalf("native control: %v", err)
	}

	bundle := filepath.Join(t.TempDir(), "graph.icebug")
	be := backendConn{native}
	if _, err := ladybug.ExportIcebugCanonical(be, bundle,
		ladybug.IcebugOptions{StorageURI: bundle}); err != nil {
		t.Fatalf("canonical export: %v", err)
	}
	schemaRaw, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("schema: %v", err)
	}
	mountDir := t.TempDir()
	mountPath := filepath.Join(mountDir, "mounted.lbug")
	if err := MountIcebugGraph(ctx, mountPath, string(schemaRaw), nil); err != nil {
		t.Fatalf("mount: %v", err)
	}
	if raw, mErr := os.ReadFile(filepath.Join(bundle, ladybug.IcebugManifestFile)); mErr == nil {
		if wErr := os.WriteFile(filepath.Join(mountDir, ladybug.IcebugManifestFile), raw, 0o644); wErr != nil {
			t.Fatalf("stage manifest: %v", wErr)
		}
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(mountPath), IcebugDir: filepath.Join(filepath.Dir(mountPath), "graph.icebug")})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	defer func() { _ = mounted.Close() }()

	start := time.Now()
	got, err := mounted.Query(ctx, canonicalImpactQuery, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("canonical traversal: %v", err)
	}
	t.Logf("canonical 3-hop traversal over real graph: rows=%d took=%s", len(got.Records), took)
	if !uidSetsEqual(got, want) {
		t.Fatalf("canonical set differs from native:\ncanonical=%v\nnative=%v",
			recordStrings(got, "uid"), recordStrings(want, "uid"))
	}
	if took > 5*time.Second {
		t.Fatalf("took %s, want <= 5s", took)
	}
}

func TestMountedCanonicalRemoteRealGraphTraversalCost(t *testing.T) {
	if os.Getenv("GRAPHIT_REMOTE_ICEBUG_TEST") == "" {
		t.Skip("set GRAPHIT_REMOTE_ICEBUG_TEST=1 with a configured Hub bucket")
	}
	globalDir := os.Getenv("GRAPHIT_REMOTE_GLOBAL_DIR")
	if globalDir == "" || os.Getenv("GRAPHIT_REAL_STORE") == "" {
		t.Skip("set GRAPHIT_REMOTE_GLOBAL_DIR and GRAPHIT_REAL_STORE")
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	ctx := context.Background()

	objectStore, err := s3store.New(ctx, config.HubS3Config())
	if err != nil {
		t.Fatalf("object store: %v", err)
	}
	prefix := s3store.JoinKey("diagnostics", "icebug-canonical-"+strconv.FormatInt(time.Now().UnixNano(), 10))
	t.Cleanup(func() {
		if dErr := objectStore.DeletePrefix(context.Background(), prefix); dErr != nil {
			t.Errorf("delete remote diagnostic prefix: %v", dErr)
		}
	})

	native := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(os.Getenv("GRAPHIT_REAL_STORE")), IcebugDir: filepath.Join(filepath.Dir(os.Getenv("GRAPHIT_REAL_STORE")), "graph.icebug")})
	if err := native.connect(); err != nil {
		t.Fatalf("open native: %v", err)
	}
	want, err := native.Query(ctx, canonicalImpactQuery, nil)
	if err != nil {
		t.Fatalf("native control: %v", err)
	}
	_ = native.Close()

	bundle := filepath.Join(t.TempDir(), "graph.icebug")
	if _, err := exportGraphToIcebug(os.Getenv("GRAPHIT_REAL_STORE"), bundle,
		"", objectStore.URI(prefix), nil); err != nil {
		t.Fatalf("export: %v", err)
	}
	if err := objectStore.UploadDir(ctx, bundle, prefix); err != nil {
		t.Fatalf("upload: %v", err)
	}
	schemaRaw, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("schema: %v", err)
	}
	mountDir := t.TempDir()
	mountPath := filepath.Join(mountDir, "mounted.lbug")
	if err := MountIcebugGraph(ctx, mountPath, string(schemaRaw), nil); err != nil {
		t.Fatalf("mount: %v", err)
	}
	if raw, mErr := os.ReadFile(filepath.Join(bundle, ladybug.IcebugManifestFile)); mErr == nil {
		if wErr := os.WriteFile(filepath.Join(mountDir, ladybug.IcebugManifestFile), raw, 0o644); wErr != nil {
			t.Fatalf("stage manifest: %v", wErr)
		}
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(mountPath), IcebugDir: filepath.Join(filepath.Dir(mountPath), "graph.icebug")})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	defer func() { _ = mounted.Close() }()

	start := time.Now()
	got, err := mounted.Query(ctx, canonicalImpactQuery, nil)
	took := time.Since(start)
	if err != nil {
		t.Fatalf("remote canonical traversal: %v", err)
	}
	t.Logf("canonical 3-hop over S3: rows=%d took=%s", len(got.Records), took)
	if !uidSetsEqual(got, want) {
		t.Fatalf("remote set differs from native")
	}
	if took > 10*time.Second {
		t.Fatalf("took %s, want <= 10s", took)
	}
}

// TestMountedCanonicalS3Battery runs repeated on-the-fly traversals against a REAL MinIO
// bucket and compares every round against the local native store. The published bundle is
// deliberately LEFT ON THE BUCKET under diagnostics/ so the operator can inspect it later;
// the full URI is printed as KEEP-URL.
func TestMountedCanonicalS3Battery(t *testing.T) {
	if os.Getenv("GRAPHIT_REMOTE_ICEBUG_TEST") == "" {
		t.Skip("set GRAPHIT_REMOTE_ICEBUG_TEST=1 with a configured Hub bucket")
	}
	globalDir := os.Getenv("GRAPHIT_REMOTE_GLOBAL_DIR")
	storePath := os.Getenv("GRAPHIT_REAL_STORE")
	if globalDir == "" || storePath == "" {
		t.Skip("set GRAPHIT_REMOTE_GLOBAL_DIR and GRAPHIT_REAL_STORE")
	}
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), globalDir)
	ctx := context.Background()

	objectStore, err := s3store.New(ctx, config.HubS3Config())
	if err != nil {
		t.Fatalf("object store: %v", err)
	}
	if err := objectStore.EnsureBucket(ctx); err != nil {
		t.Fatalf("bucket: %v", err)
	}
	prefix := s3store.JoinKey("diagnostics",
		fmt.Sprintf("t18-canonical-battery-%d", time.Now().UnixNano()))
	t.Cleanup(func() {})

	native := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(storePath), IcebugDir: filepath.Join(filepath.Dir(storePath), "graph.icebug")})
	if err := native.connect(); err != nil {
		t.Fatalf("native: %v", err)
	}
	want, err := native.Query(ctx, canonicalImpactQuery, nil)
	if err != nil {
		t.Fatalf("native control: %v", err)
	}
	nativeStart := time.Now()
	for i := 0; i < 6; i++ {
		if _, err := native.Query(ctx, canonicalImpactQuery, nil); err != nil {
			t.Fatalf("native round %d: %v", i, err)
		}
	}
	t.Logf("LOCAL native x6 total=%s", time.Since(nativeStart))

	bundle := filepath.Join(t.TempDir(), "graph.icebug")
	if _, err := exportGraphToIcebug(storePath, bundle, "", objectStore.URI(prefix), nil); err != nil {
		t.Fatalf("export canonical to S3: %v", err)
	}
	if err := objectStore.UploadDir(ctx, bundle, prefix); err != nil {
		t.Fatalf("upload: %v", err)
	}
	schemaRaw, err := os.ReadFile(filepath.Join(bundle, "schema.cypher"))
	if err != nil {
		t.Fatalf("schema: %v", err)
	}
	mountDir := t.TempDir()
	mountPath := filepath.Join(mountDir, "mounted.lbug")
	if err := MountIcebugGraph(ctx, mountPath, string(schemaRaw), nil); err != nil {
		t.Fatalf("mount: %v", err)
	}
	if raw, mErr := os.ReadFile(filepath.Join(bundle, ladybug.IcebugManifestFile)); mErr == nil {
		if wErr := os.WriteFile(filepath.Join(mountDir, ladybug.IcebugManifestFile), raw, 0o644); wErr != nil {
			t.Fatalf("stage manifest: %v", wErr)
		}
	}
	mounted := NewLadybugDBReadOnly(LadybugConfig{StoreDir: filepath.Dir(mountPath), IcebugDir: filepath.Join(filepath.Dir(mountPath), "graph.icebug")})
	if err := mounted.connect(); err != nil {
		t.Fatalf("open mounted: %v", err)
	}
	defer func() { _ = mounted.Close() }()

	var total time.Duration
	for round := 1; round <= 6; round++ {
		start := time.Now()
		got, qerr := mounted.Query(ctx, canonicalImpactQuery, nil)
		took := time.Since(start)
		total += took
		if qerr != nil {
			t.Fatalf("S3 round %d: %v", round, qerr)
		}
		if !uidSetsEqual(got, want) {
			t.Fatalf("S3 round %d set differs from native", round)
		}
		t.Logf("S3  round %d rows=%d took=%s", round, len(got.Records), took)
	}
	t.Logf("KEEP-URL %s (endpoint %s)", objectStore.URI(prefix), config.HubS3Config().Endpoint)
	t.Logf("S3 x6 total=%s avg=%s | native control set: %d callers",
		total, total/6, len(recordStrings(want, "uid")))
}

func TestCanonicalMembersExpandSmallestFirst(t *testing.T) {
	g := &ladybug.CanonicalRelGroup{Type: "CALLS", Members: []ladybug.CanonicalMember{
		{From: "Function", To: "Function", Table: "big", Rows: 46201},
		{From: "Method", To: "Method", Table: "small", Rows: 1222},
		{From: "Function", To: "Method", Table: "mid", Rows: 3553},
	}}
	m := &ladybug.CanonicalManifest{NodeTables: []ladybug.CanonicalNodeTable{
		{Label: "Function", Columns: []ladybug.Field{{Name: "uid"}}},
		{Label: "Method", Columns: []ladybug.Field{{Name: "uid"}}},
	}}
	out := canonicalUIDMembers(m, g, false, false)
	if len(out) != 3 || out[0].Table != "small" || out[1].Table != "mid" || out[2].Table != "big" {
		t.Fatalf("expansion order = %+v, want small,mid,big", out)
	}
}
