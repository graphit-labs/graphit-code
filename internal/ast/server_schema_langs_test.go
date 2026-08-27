package ast

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"strings"
	"testing"
)

// The explorer's Schema panel used to list every label in one flat run — Function next
// to CssClass next to Heading — with no way to tell which language contributed which.
// /api/schema now also groups the counts per language, and this is that grouping.
func TestSchemaLangGroupsGroupsLabelsByLanguage(t *testing.T) {
	t.Parallel()

	db := NewLadybugDB(LadybugConfig{DBPath: filepath.Join(t.TempDir(), "ladybugdb")})
	if err := db.connect(); err != nil {
		t.Skipf("ladybug unavailable: %v", err)
	}
	defer func() { _ = db.Close() }()

	ctx := context.Background()
	if err := db.initSchemaForLabels(SchemaInfo{Labels: []string{"Function", "Comment"}}); err != nil {
		t.Fatalf("schema: %v", err)
	}

	seed := []string{
		`CREATE (:Function {uid:'g1', name:'A', lang:'go'})`,
		`CREATE (:Function {uid:'g2', name:'B', lang:'go'})`,
		`CREATE (:Function {uid:'g3', name:'C', lang:'go'})`,
		`CREATE (:Comment {uid:'g4', name:'// note', lang:'go'})`,
		`CREATE (:Comment {uid:'g5', name:'// other', lang:'go'})`,
		`CREATE (:Function {uid:'t1', name:'D', lang:'tsx'})`,
		// Call-target stubs carry no lang — they are what the "(no language)" group holds.
		`CREATE (:Function {uid:'s1', name:'E'})`,
		`CREATE (:Function {uid:'s2', name:'F'})`,
	}
	for _, q := range seed {
		if _, err := db.Execute(ctx, q, nil); err != nil {
			t.Fatalf("seed %q: %v", q, err)
		}
	}

	groups := schemaLangGroups(ctx, db)

	// go (5 nodes) outranks tsx (1); the language-less group is last even though its
	// 2 nodes beat tsx — it is not a language, so it never competes on count.
	wantOrder := []string{"go", "tsx", ""}
	if len(groups) != len(wantOrder) {
		t.Fatalf("got %d groups, want %d: %v", len(groups), len(wantOrder), groups)
	}
	for i, want := range wantOrder {
		if got := groups[i]["lang"]; got != want {
			t.Errorf("group %d is %q, want %q", i, got, want)
		}
	}

	if got := toInt(groups[0]["count"]); got != 5 {
		t.Errorf("go totals %d nodes, want 5", got)
	}
	if got := toInt(groups[2]["count"]); got != 2 {
		t.Errorf("the language-less group totals %d nodes, want 2", got)
	}

	// Within a group, only that language's labels, biggest first.
	goLabels, ok := groups[0]["labels"].([]map[string]any)
	if !ok {
		t.Fatalf("go labels are %T, want []map[string]any", groups[0]["labels"])
	}
	if len(goLabels) != 2 {
		t.Fatalf("go has %d labels, want 2: %v", len(goLabels), goLabels)
	}
	if goLabels[0]["label"] != "Function" || toInt(goLabels[0]["count"]) != 3 {
		t.Errorf("go's first label is %v, want Function x3", goLabels[0])
	}
	if goLabels[1]["label"] != "Comment" || toInt(goLabels[1]["count"]) != 2 {
		t.Errorf("go's second label is %v, want Comment x2", goLabels[1])
	}

	// The same label appears under every language that produced it — the grouping
	// splits the flat count, it does not pick one owner per label.
	var functions int
	for _, g := range groups {
		labels, _ := g["labels"].([]map[string]any)
		for _, l := range labels {
			if l["label"] == "Function" {
				functions += toInt(l["count"])
			}
		}
	}
	if functions != 6 {
		t.Errorf("Function totals %d across the groups, want 6", functions)
	}
}

// noLangPropertyDB answers the label and edge queries, and fails the one that reads
// `n.lang` — the shape of a graph caught mid-rebuild, whose partial schema has the
// property on no table at all.
type noLangPropertyDB struct {
	emptyGraphDB
}

func (d *noLangPropertyDB) Query(_ context.Context, q string, _ map[string]any) (*QueryResult, error) {
	if strings.Contains(q, "n.lang") {
		return nil, errors.New("Binder exception: Cannot find property lang for n")
	}
	if strings.Contains(q, "label(n)") {
		return &QueryResult{Records: []QueryRecord{{"label": "Function", "count": int64(7)}}}, nil
	}
	return &QueryResult{}, nil
}

// The grouping is an addition to /api/schema, so it must never cost the panel the rest
// of the response: a graph that cannot answer the lang query still gets its labels.
func TestSchemaEndpointStillAnswersWhenTheLangQueryFails(t *testing.T) {
	t.Parallel()

	s := &Server{db: &noLangPropertyDB{}, repoPath: t.TempDir()}

	rec := httptest.NewRecorder()
	s.handleSchema(rec, httptest.NewRequest(http.MethodGet, "/api/schema", nil))

	if rec.Code != http.StatusOK {
		t.Fatalf("status %d, want 200 — the lang grouping must degrade, not fail the request: %s", rec.Code, rec.Body.String())
	}

	var body struct {
		Nodes []map[string]any `json:"nodes"`
		Langs []map[string]any `json:"langs"`
	}
	if err := json.Unmarshal(rec.Body.Bytes(), &body); err != nil {
		t.Fatalf("decode: %v — %s", err, rec.Body.String())
	}
	if len(body.Nodes) != 1 {
		t.Errorf("got %d node stats, want the flat list to survive: %v", len(body.Nodes), body.Nodes)
	}
	if len(body.Langs) != 0 {
		t.Errorf("langs is %v, want empty so the panel falls back to the flat list", body.Langs)
	}
	if !strings.Contains(rec.Body.String(), `"langs":[]`) {
		t.Errorf("langs is missing or null; the client reads it as a list: %s", rec.Body.String())
	}
}
