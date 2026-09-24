package ast

import (
	"context"
	"encoding/json"
	"net/http/httptest"
	"net/url"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func TestGraphRelationshipIDsAndNeighborhoodKeepParallelCalls(t *testing.T) {
	ctx := context.Background()
	work := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "calls.go"), []byte("package demo\nfunc A() {\n B()\n B()\n}\nfunc B() {}\n"), 0600); err != nil {
		t.Fatal(err)
	}
	root := t.TempDir()
	cfg := LadybugConfig{StoreDir: root, IcebugDir: filepath.Join(root, "graph.icebug")}
	db := NewLadybugDB(cfg)
	if _, err := RunPipeline(ctx, db, work, PipelineOptions{CacheDir: root, SkipExternal: true}); err != nil {
		t.Fatal(err)
	}
	_ = db.Close()
	db = NewLadybugDB(cfg)
	defer func() { _ = db.Close() }()

	result, err := db.Query(ctx, "MATCH (n:Function) WHERE n.name = 'A' RETURN n.uid AS uid LIMIT 1", nil)
	if err != nil || len(result.Records) != 1 {
		t.Fatalf("locate A: %v, %v", err, result)
	}
	uid, _ := result.Records[0]["uid"].(string)
	if uid == "" {
		t.Fatal("A has no UID")
	}
	request := graphNeighborhoodRequest{anchorLabel: "Function", anchorIdentity: uid, relationshipType: "CALLS", targetLabel: "Function", direction: "outgoing", limit: 1}
	first, cursor, err := queryGraphNeighborhood(ctx, db, request)
	if err != nil || len(first) != 1 || cursor == "" {
		t.Fatalf("first relation page = %v, %q, %v", first, cursor, err)
	}
	request.cursor = cursor
	second, next, err := queryGraphNeighborhood(ctx, db, request)
	if err != nil || len(second) != 1 || next != "" {
		t.Fatalf("second relation page = %v, %q, %v", second, next, err)
	}
	firstRel := first[0].record["r"].(map[string]any)
	secondRel := second[0].record["r"].(map[string]any)
	firstID, secondID := ladybugIDStr(firstRel["ID"]), ladybugIDStr(secondRel["ID"])
	if firstID == "" || secondID == "" || firstID == secondID {
		t.Fatalf("parallel relations have IDs %q and %q", firstID, secondID)
	}
	if first[0].uid == "" || second[0].uid == "" || first[0].uid == second[0].uid {
		t.Fatalf("parallel relations have UIDs %q and %q", first[0].uid, second[0].uid)
	}
	server := &Server{db: db}
	queryParams := url.Values{
		"anchor_label": {"Function"}, "anchor_identity": {uid}, "relationship_type": {"CALLS"},
		"target_label": {"Function"}, "direction": {"outgoing"}, "limit": {"1"},
	}
	response := httptest.NewRecorder()
	server.handleGraphNeighborhood(response, httptest.NewRequest("GET", "/api/graph/neighborhood?"+queryParams.Encode(), nil))
	var page struct {
		Links           []map[string]any `json:"links"`
		NextCursor      string           `json:"next_cursor"`
		IndexGeneration string           `json:"index_generation"`
	}
	if response.Code != 200 || json.Unmarshal(response.Body.Bytes(), &page) != nil || len(page.Links) != 1 || page.Links[0]["uid"] == "" || page.NextCursor == "" || page.IndexGeneration == "" {
		t.Fatalf("neighborhood HTTP response: %d %s", response.Code, response.Body.String())
	}
	queryParams.Set("anchor_identity", "calls.go::B")
	queryParams.Set("direction", "incoming")
	queryParams.Set("limit", "2")
	incoming := httptest.NewRecorder()
	server.handleGraphNeighborhood(incoming, httptest.NewRequest("GET", "/api/graph/neighborhood?"+queryParams.Encode(), nil))
	var inbound struct {
		Links []map[string]any `json:"links"`
	}
	if incoming.Code != 200 || json.Unmarshal(incoming.Body.Bytes(), &inbound) != nil || len(inbound.Links) != 2 {
		t.Fatalf("incoming neighborhood: %d %s", incoming.Code, incoming.Body.String())
	}
	queryParams.Set("limit", "0")
	invalid := httptest.NewRecorder()
	server.handleGraphNeighborhood(invalid, httptest.NewRequest("GET", "/api/graph/neighborhood?"+queryParams.Encode(), nil))
	if invalid.Code != 400 {
		t.Fatalf("invalid limit accepted: %d %s", invalid.Code, invalid.Body.String())
	}
	queryParams.Set("limit", "1")
	queryParams.Set("anchor_label", "Bad-Label")
	invalidLabel := httptest.NewRecorder()
	server.handleGraphNeighborhood(invalidLabel, httptest.NewRequest("GET", "/api/graph/neighborhood?"+queryParams.Encode(), nil))
	if invalidLabel.Code != 400 {
		t.Fatalf("invalid label accepted: %d %s", invalidLabel.Code, invalidLabel.Body.String())
	}
	queryParams.Set("anchor_label", "Function")
	queryParams.Set("cursor", "not-a-relation-uid")
	invalidCursor := httptest.NewRecorder()
	server.handleGraphNeighborhood(invalidCursor, httptest.NewRequest("GET", "/api/graph/neighborhood?"+queryParams.Encode(), nil))
	if invalidCursor.Code != 400 {
		t.Fatalf("invalid cursor accepted: %d %s", invalidCursor.Code, invalidCursor.Body.String())
	}
	queryParams.Del("cursor")
	ordered, orderedErr := db.Query(ctx, "MATCH (a:Function)-[r:calls__function_function]->(b:Function) WHERE a.uid = 'calls.go::A' AND r.uid > '' RETURN a,r,b ORDER BY r.uid LIMIT 2", nil)
	if orderedErr != nil || len(ordered.Records) != 2 {
		t.Fatalf("bounded relation UID query: %v %v", ordered, orderedErr)
	}
	for _, row := range []graphNeighborhoodRow{first[0], second[0]} {
		nodes := map[string]map[string]any{}
		var links []map[string]any
		extractUserQueryGraph(row.record, nodes, &links)
		if len(links) != 1 || links[0]["uid"] != row.uid || links[0]["source"] == "" || links[0]["target"] == "" {
			t.Fatalf("stable relation identity lost: %v", links)
		}
	}
	sampleResponse := httptest.NewRecorder()
	server.handleGraph(sampleResponse, httptest.NewRequest("GET", "/api/graph", nil))
	var sampleGraph struct {
		Links []map[string]any `json:"links"`
	}
	if sampleResponse.Code != 200 || json.Unmarshal(sampleResponse.Body.Bytes(), &sampleGraph) != nil {
		t.Fatalf("/api/graph sample: %d %s", sampleResponse.Code, sampleResponse.Body.String())
	}
	var callLinks []map[string]any
	for _, link := range sampleGraph.Links {
		if link["type"] == "CALLS" {
			callLinks = append(callLinks, link)
		}
	}
	if len(callLinks) != 2 || callLinks[0]["uid"] == "" || callLinks[1]["uid"] == "" || callLinks[0]["uid"] == callLinks[1]["uid"] || callLinks[0]["id"] == "" || callLinks[1]["id"] == "" || callLinks[0]["id"] == callLinks[1]["id"] {
		t.Fatalf("/api/graph sample lost parallel relation identity: %v", callLinks)
	}

	sample, err := queryGraphEdgeSample(ctx, db)
	if err != nil {
		t.Fatal(err)
	}
	var ids, uids []string
	for _, record := range sample.Records {
		if !strings.EqualFold(safeStr(record["rel_type"]), "calls__function_function") {
			continue
		}
		if safeStr(record["src_uid"]) == uid && safeStr(record["dst_name"]) == "B" {
			ids = append(ids, ladybugIDStr(record["rel_id"]))
			uids = append(uids, safeStr(record["rel_uid"]))
		}
	}
	if len(ids) != 2 || ids[0] == "" || ids[1] == "" || ids[0] == ids[1] {
		t.Fatalf("sample lost parallel relation IDs: %v", ids)
	}
	if len(uids) != 2 || uids[0] == "" || uids[1] == "" || uids[0] == uids[1] {
		t.Fatalf("sample lost parallel relation UIDs: %v", uids)
	}
	triples, err := db.Query(ctx, "MATCH (n:Function)-[r:CALLS]->(m:Function) WHERE n.uid = 'calls.go::A' RETURN n,r,m LIMIT 2", nil)
	if err != nil || len(triples.Records) != 2 {
		t.Fatalf("logical RETURN n,r,m: %v, %v", triples, err)
	}
	for _, record := range triples.Records {
		for _, variable := range []string{"n", "r", "m"} {
			value, ok := record[variable].(map[string]any)
			props, _ := value["Properties"].(map[string]any)
			if !ok || safeStr(props["uid"]) == "" {
				t.Fatalf("RETURN %s omitted UID: %v", variable, record)
			}
		}
	}
	physical, err := db.Query(ctx, "MATCH (n:Function)-[r:calls__function_function]->(m:Function) WHERE n.uid = 'calls.go::A' RETURN r LIMIT 10", nil)
	if err != nil || len(physical.Records) != 2 {
		t.Fatalf("physical comparison query: %v %v", physical, err)
	}
	physicalIDs := map[string]bool{}
	for _, row := range physical.Records {
		physicalIDs[ladybugIDStr(row["r"].(map[string]any)["ID"])] = true
	}
	for _, row := range triples.Records {
		rel := row["r"].(map[string]any)
		if rel["Label"] != "CALLS" || !physicalIDs[ladybugIDStr(rel["ID"])] {
			t.Fatalf("logical relation did not preserve physical identity: %v", rel)
		}
	}
	graphQuery := "MATCH (n:Function)-[r:CALLS]->(m:Function) WHERE n.uid = 'calls.go::A' RETURN n,r,m LIMIT 10"
	graphResponse := httptest.NewRecorder()
	server.handleGraph(graphResponse, httptest.NewRequest("GET", "/api/graph?cypher_query="+url.QueryEscape(graphQuery), nil))
	var graph struct {
		Links []map[string]any `json:"links"`
	}
	if graphResponse.Code != 200 || json.Unmarshal(graphResponse.Body.Bytes(), &graph) != nil || len(graph.Links) != 2 {
		t.Fatalf("/api/graph triples: %d %s", graphResponse.Code, graphResponse.Body.String())
	}
	if graph.Links[0]["uid"] == "" || graph.Links[1]["uid"] == "" || graph.Links[0]["uid"] == graph.Links[1]["uid"] || graph.Links[0]["id"] == "" || graph.Links[1]["id"] == "" || graph.Links[0]["id"] == graph.Links[1]["id"] {
		t.Fatalf("/api/graph lost relation identities: %v", graph.Links)
	}
	for _, projection := range []string{"n", "r", "m"} {
		query := "MATCH (n:Function)-[r:CALLS]->(m:Function) WHERE n.uid = 'calls.go::A' RETURN " + projection + " LIMIT 10"
		one, err := db.Query(ctx, query, nil)
		if err != nil || len(one.Records) != 2 {
			t.Fatalf("logical RETURN %s: %v, %v", projection, one, err)
		}
		for _, record := range one.Records {
			value, ok := record[projection].(map[string]any)
			props, _ := value["Properties"].(map[string]any)
			if !ok || safeStr(props["uid"]) == "" || len(record) != 1 {
				t.Fatalf("RETURN %s result lost identity: %v", projection, record)
			}
		}
	}
	for _, query := range []string{"MATCH (n)-[r]-(m) RETURN n,r,m LIMIT 10", "MATCH (n)-[r]-(m) RETURN n,r,m"} {
		untyped, err := db.Query(ctx, query, nil)
		if err != nil || len(untyped.Records) < 4 {
			t.Fatalf("untyped RETURN n,r,m: %v, %v", untyped, err)
		}
		callOrientations := 0
		for _, record := range untyped.Records {
			if rel, ok := record["r"].(map[string]any); ok && rel["Label"] == "CALLS" {
				callOrientations++
			}
			for _, variable := range []string{"n", "r", "m"} {
				value, ok := record[variable].(map[string]any)
				props, _ := value["Properties"].(map[string]any)
				identity := safeStr(props["uid"])
				if variable != "r" && identity == "" {
					identity = safeStr(props["path"])
				}
				if !ok || identity == "" {
					t.Fatalf("untyped RETURN %s omitted identity: %v", variable, record)
				}
			}
		}
		if callOrientations != 4 {
			t.Fatalf("undirected CALLS should expose two orientations of each parallel relation: %d", callOrientations)
		}
	}
	pageOfTriples, err := db.QueryPage(ctx, "MATCH (n)-[r]-(m) RETURN n,r,m", nil, 1, 2)
	if err != nil || len(pageOfTriples.Records) != 2 {
		t.Fatalf("bounded untyped triple page: %v %v", pageOfTriples, err)
	}
	if _, err := db.Query(ctx, "MATCH (n)-[r]-(m) RETURN n,r,m LIMIT 1001", nil); err == nil || !strings.Contains(err.Error(), "exceeds 1000") {
		t.Fatalf("oversized triple projection should require a bound: %v", err)
	}

	// Rebuild a different file. The native relation ID may be reassigned by the
	// mounted bundle; the cursor and both source relation UIDs must still work.
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
	other := filepath.Join(work, "0unrelated.go")
	if err := os.WriteFile(other, []byte("package demo\nfunc C() {}\n"), 0600); err != nil {
		t.Fatal(err)
	}
	updateDB := NewLadybugDB(cfg)
	if _, err := RunPipelineForPaths(ctx, updateDB, work, []string{other}, nil, PipelineOptions{CacheDir: root, SkipExternal: true}); err != nil {
		t.Fatalf("incremental reindex: %v", err)
	}
	if err := updateDB.Close(); err != nil {
		t.Fatal(err)
	}
	db = NewLadybugDB(cfg)
	server.db = db
	queryParams.Set("anchor_identity", uid)
	queryParams.Set("direction", "outgoing")
	queryParams.Set("limit", "1")
	after := httptest.NewRecorder()
	server.handleGraphNeighborhood(after, httptest.NewRequest("GET", "/api/graph/neighborhood?"+queryParams.Encode(), nil))
	var newPage struct {
		IndexGeneration string `json:"index_generation"`
	}
	if after.Code != 200 || json.Unmarshal(after.Body.Bytes(), &newPage) != nil || newPage.IndexGeneration == "" || newPage.IndexGeneration == page.IndexGeneration {
		t.Fatalf("reindex generation did not change: before=%q after=%d %s", page.IndexGeneration, after.Code, after.Body.String())
	}
	replay := request
	replay.cursor = ""
	replayedFirst, replayCursor, err := queryGraphNeighborhood(ctx, db, replay)
	if err != nil || len(replayedFirst) != 1 || replayedFirst[0].uid != first[0].uid || replayCursor != cursor {
		t.Fatalf("first page changed after unrelated reindex: %v %q %v", replayedFirst, replayCursor, err)
	}
	replayedSecond, _, err := queryGraphNeighborhood(ctx, db, request)
	if err != nil || len(replayedSecond) != 1 || replayedSecond[0].uid != second[0].uid {
		t.Fatalf("UID cursor failed after unrelated reindex: %v %v", replayedSecond, err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.Remove(filepath.Join(work, "calls.go")); err != nil {
		t.Fatal(err)
	}
	deleteDB := NewLadybugDB(cfg)
	if _, err := RunPipelineForPaths(ctx, deleteDB, work, nil, []string{"calls.go"}, PipelineOptions{CacheDir: root, SkipExternal: true}); err != nil {
		t.Fatalf("delete reindex: %v", err)
	}
	if err := deleteDB.Close(); err != nil {
		t.Fatal(err)
	}
	db = NewLadybugDB(cfg)
	if _, _, err := queryGraphNeighborhood(ctx, db, replay); err == nil || !strings.Contains(err.Error(), "no longer exists") {
		t.Fatalf("deleted selected entity should be recoverable: %v", err)
	}
}

func TestStableRelationshipUIDDistinguishesIdenticalOccurrences(t *testing.T) {
	row := map[string]any{"caller_uid": "a", "callee_uid": "b", "line_number": 7}
	occurrences := map[string]int{}
	first := stableRelationshipUID("CALLS", "Function", "Function", row, occurrences)
	second := stableRelationshipUID("CALLS", "Function", "Function", row, occurrences)
	if first == "" || second == "" || first == second {
		t.Fatalf("identical indexed occurrences require distinct UIDs: %q %q", first, second)
	}
	replay := map[string]int{}
	if got := stableRelationshipUID("CALLS", "Function", "Function", row, replay); got != first {
		t.Fatalf("same indexed occurrence changed UID: %q -> %q", first, got)
	}
}

func TestIncrementalReindexRemovesLastPerFileFunctionAndCall(t *testing.T) {
	ctx := context.Background()
	work := t.TempDir()
	for path, source := range map[string]string{
		"changed.go": "package demo\nfunc A(){B()}\nfunc B(){}\n",
		"kept.go":    "package demo\nfunc C(){D()}\nfunc D(){}\n",
	} {
		if err := os.WriteFile(filepath.Join(work, path), []byte(source), 0600); err != nil {
			t.Fatal(err)
		}
	}
	root := t.TempDir()
	cfg := LadybugConfig{StoreDir: root, IcebugDir: filepath.Join(root, "graph.icebug")}
	db := NewLadybugDB(cfg)
	if _, err := RunPipeline(ctx, db, work, PipelineOptions{CacheDir: root, SkipExternal: true}); err != nil {
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(work, "changed.go"), []byte("package demo\n"), 0600); err != nil {
		t.Fatal(err)
	}
	db = NewLadybugDB(cfg)
	if _, err := RunPipelineForPaths(ctx, db, work, []string{"changed.go"}, nil, PipelineOptions{CacheDir: root, SkipExternal: true}); err != nil {
		t.Fatalf("incremental reindex: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
	db = NewLadybugDB(cfg)
	defer db.Close()
	old, err := db.Query(ctx, "MATCH (n:Function) WHERE n.uid = 'changed.go::A' RETURN n.uid AS uid LIMIT 1", nil)
	if err != nil || len(old.Records) != 0 {
		t.Fatalf("removed function retained: %v %v", old, err)
	}
	remaining, err := db.Query(ctx, "MATCH (n:Function)-[r:CALLS]->(m:Function) RETURN n,r,m LIMIT 10", nil)
	if err != nil || len(remaining.Records) != 1 {
		t.Fatalf("removed relation retained or unrelated one lost: %v %v", remaining, err)
	}
	caller := remaining.Records[0]["n"].(map[string]any)
	callerProperties := caller["Properties"].(map[string]any)
	if callerProperties["uid"] != "kept.go::C" {
		t.Fatalf("unexpected surviving caller: %v", callerProperties)
	}
}

func TestLegacyRelationBundleRequiresReindexForNavigation(t *testing.T) {
	ctx := context.Background()
	work := t.TempDir()
	if err := os.WriteFile(filepath.Join(work, "calls.go"), []byte("package demo\nfunc A(){B()}\nfunc B(){}\n"), 0600); err != nil {
		t.Fatal(err)
	}
	root := t.TempDir()
	cfg := LadybugConfig{StoreDir: root, IcebugDir: filepath.Join(root, "graph.icebug")}
	db := NewLadybugDB(cfg)
	if _, err := RunPipeline(ctx, db, work, PipelineOptions{CacheDir: root, SkipExternal: true}); err != nil {
		t.Fatal(err)
	}
	if err := db.Close(); err != nil {
		t.Fatal(err)
	}
	manifestPath := filepath.Join(cfg.IcebugDir, "icebug.json")
	raw, err := os.ReadFile(manifestPath)
	if err != nil {
		t.Fatal(err)
	}
	var manifest map[string]any
	if err := json.Unmarshal(raw, &manifest); err != nil {
		t.Fatal(err)
	}
	manifest["relation_uids"] = false
	raw, err = json.Marshal(manifest)
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(manifestPath, raw, 0600); err != nil {
		t.Fatal(err)
	}
	db = NewLadybugDB(cfg)
	defer db.Close()
	request := graphNeighborhoodRequest{anchorLabel: "Function", anchorIdentity: "calls.go::A", relationshipType: "CALLS", targetLabel: "Function", direction: "outgoing", limit: 1}
	if _, _, err := queryGraphNeighborhood(ctx, db, request); err == nil || !strings.Contains(err.Error(), "reindex") {
		t.Fatalf("legacy neighborhood should require reindex: %v", err)
	}
	if _, err := queryGraphEdgeSample(ctx, db); err == nil || !strings.Contains(err.Error(), "reindex") {
		t.Fatalf("legacy sample should require reindex: %v", err)
	}
}
