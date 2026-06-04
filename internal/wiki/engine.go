package wiki

import (
	"context"
	"crypto/sha256"
	"fmt"
	"math"
	"math/rand"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"time"
)

type Record map[string]any

type QueryResult struct {
	Records []Record
}

type GraphAdapter interface {
	Query(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error)
	Execute(ctx context.Context, cypher string, params map[string]any) (*QueryResult, error)
	Close() error
}

type Node struct {
	UID     string
	Label   string
	Name    string
	Path    string
	Hash    string
	Summary string
	Extra   map[string]any
}

type Edge struct {
	SrcUID    string
	DstUID    string
	EdgeLabel string
	Weight    float64
	Extra     map[string]any
}

type ExtractionResult struct {
	Nodes []Node
	Edges []Edge
}

type Extractor interface {
	Extract(relPath, content string) *ExtractionResult
}

type GraphWriter interface {
	WriteResult(ctx context.Context, result *ExtractionResult) error
	DeleteDocument(ctx context.Context, path string) error
	Stats() WriteStats
}

type WikiRenderer interface {
	EntityPage(ctx context.Context, db GraphAdapter, entity EntitySummary) string

	IndexPage(entities []EntitySummary, communities []Community, godNodes []map[string]any, totalNodes, totalEdges int, moduleTag string) string

	LogTitle() string

	ModuleTag() string
}

type EntitySummary struct {
	UID     string
	Name    string
	Path    string
	Summary string
	Type    string
}

type WriteStats struct {
	NodesWritten int
	EdgesWritten int
}

type IgnoreConfig struct {
	Filename string

	ExtraPatterns []string

	SupportedExts map[string]bool

	MaxFileSizeBytes int64
}

func CollectFiles(rootPath string, cfg IgnoreConfig) ([]string, error) {
	ignorePath := findIgnoreFile(rootPath, cfg.Filename)
	patterns := readIgnorePatterns(ignorePath, cfg.ExtraPatterns)

	var files []string
	err := filepath.Walk(rootPath, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return nil
		}
		relPath, _ := filepath.Rel(rootPath, path)
		relPath = filepath.ToSlash(relPath)

		if info.IsDir() {
			name := info.Name()
			if strings.HasPrefix(name, ".") && name != "." {
				return filepath.SkipDir
			}
			if matchIgnore(relPath+"/", patterns) {
				return filepath.SkipDir
			}
			return nil
		}

		if matchIgnore(relPath, patterns) {
			return nil
		}

		if len(cfg.SupportedExts) > 0 {
			ext := strings.ToLower(filepath.Ext(path))
			if !cfg.SupportedExts[ext] {
				return nil
			}
		}

		if cfg.MaxFileSizeBytes > 0 && info.Size() > cfg.MaxFileSizeBytes {
			return nil
		}

		files = append(files, path)
		return nil
	})
	return files, err
}

func EnsureIgnoreFile(rootPath string, cfg IgnoreConfig, header string) {
	if cfg.Filename == "" {
		return
	}
	dest := filepath.Join(rootPath, cfg.Filename)
	if _, err := os.Stat(dest); err == nil {
		return
	}
	_ = os.MkdirAll(filepath.Dir(dest), 0o755)
	_ = os.WriteFile(dest, []byte(header+"\n"), 0o644)
}

func findIgnoreFile(root, filename string) string {
	if filename == "" {
		return ""
	}
	candidate := filepath.Join(root, filename)
	if _, err := os.Stat(candidate); err == nil {
		return candidate
	}
	return ""
}

func readIgnorePatterns(path string, defaults []string) []string {
	var user []string
	if path != "" {
		data, err := os.ReadFile(path)
		if err == nil {
			for _, line := range strings.Split(string(data), "\n") {
				line = strings.TrimSpace(line)
				if line == "" || strings.HasPrefix(line, "#") {
					continue
				}
				user = append(user, line)
			}
		}
	}
	return append(user, defaults...)
}

func matchIgnore(relPath string, patterns []string) bool {
	relPath = filepath.ToSlash(relPath)
	for _, pattern := range patterns {
		pattern = strings.TrimSpace(pattern)
		if pattern == "" {
			continue
		}
		if strings.HasSuffix(pattern, "/") {
			dir := strings.TrimSuffix(pattern, "/")
			for _, part := range strings.Split(relPath, "/") {
				if ok, _ := filepath.Match(dir, part); ok {
					return true
				}
			}
			continue
		}
		base := filepath.Base(relPath)
		if ok, _ := filepath.Match(pattern, base); ok {
			return true
		}
		if ok, _ := filepath.Match(pattern, relPath); ok {
			return true
		}
	}
	return false
}

type IndexConfig struct {
	Workers    int
	Reset      bool
	BatchSize  int
	UseLouvain bool

	NodeLabels []string

	EdgeLabels []string

	RootDocNodeLabel string

	IgnoreCfg IgnoreConfig
}

type IndexResult struct {
	TotalFiles   int
	IndexedFiles int
	SkippedFiles int
	PrunedFiles  int
	NodesCreated int
	EdgesCreated int
	TotalTime    time.Duration
	ExtractTime  time.Duration
	WriteTime    time.Duration
}

func RunIndexPipeline(
	ctx context.Context,
	db GraphAdapter,
	writer GraphWriter,
	extractor Extractor,
	rootPath string,
	cfg IndexConfig,
) (*IndexResult, error) {
	start := time.Now()
	result := &IndexResult{}

	absRoot, err := filepath.Abs(rootPath)
	if err != nil {
		return nil, fmt.Errorf("invalid root path: %w", err)
	}

	if cfg.Reset {
		for _, label := range cfg.NodeLabels {
			q := fmt.Sprintf("MATCH (n:`%s`) DETACH DELETE n", label)
			_, _ = db.Execute(ctx, q, nil)
		}
	}

	files, err := CollectFiles(absRoot, cfg.IgnoreCfg)
	if err != nil {
		return nil, fmt.Errorf("collecting files: %w", err)
	}
	result.TotalFiles = len(files)

	existingHashes := loadHashes(ctx, db, cfg.RootDocNodeLabel)

	extractStart := time.Now()
	indexedPaths := make(map[string]bool)

	for _, file := range files {
		select {
		case <-ctx.Done():
			return result, ctx.Err()
		default:
		}

		relPath, _ := filepath.Rel(absRoot, file)
		relPath = filepath.ToSlash(relPath)
		indexedPaths[relPath] = true

		content, err := os.ReadFile(file)
		if err != nil {
			continue
		}

		hash := fmt.Sprintf("%x", sha256.Sum256(content))[:16]
		if existing, ok := existingHashes[relPath]; ok && existing == hash {
			result.SkippedFiles++
			continue
		}

		extraction := extractor.Extract(relPath, string(content))
		if extraction == nil || len(extraction.Nodes) == 0 {
			continue
		}

		writeStart := time.Now()
		if err := writer.WriteResult(ctx, extraction); err != nil {
			continue
		}
		result.WriteTime += time.Since(writeStart)
		result.IndexedFiles++
	}

	result.ExtractTime = time.Since(extractStart) - result.WriteTime

	if !cfg.Reset {
		pruned := pruneDeleted(ctx, db, writer, indexedPaths, cfg.RootDocNodeLabel)
		result.PrunedFiles = pruned
	}

	stats := writer.Stats()
	result.NodesCreated = stats.NodesWritten
	result.EdgesCreated = stats.EdgesWritten
	result.TotalTime = time.Since(start)

	return result, nil
}

func loadHashes(ctx context.Context, db GraphAdapter, rootLabel string) map[string]string {
	hashes := make(map[string]string)
	if rootLabel == "" {
		return hashes
	}
	q := fmt.Sprintf("MATCH (n:`%s`) RETURN n.path AS path, n.content_hash AS hash", rootLabel)
	res, err := db.Query(ctx, q, nil)
	if err != nil {
		return hashes
	}
	for _, r := range res.Records {
		path, _ := r["path"].(string)
		hash, _ := r["hash"].(string)
		if path != "" && hash != "" {
			hashes[path] = hash
		}
	}
	return hashes
}

func pruneDeleted(ctx context.Context, db GraphAdapter, writer GraphWriter, current map[string]bool, rootLabel string) int {
	if rootLabel == "" {
		return 0
	}
	q := fmt.Sprintf("MATCH (n:`%s`) RETURN n.path AS path", rootLabel)
	res, err := db.Query(ctx, q, nil)
	if err != nil {
		return 0
	}
	pruned := 0
	for _, r := range res.Records {
		path, _ := r["path"].(string)
		if path != "" && !current[path] {
			_ = writer.DeleteDocument(ctx, path)
			pruned++
		}
	}
	return pruned
}

type Algorithm int

const (
	AlgoLabelPropagation Algorithm = iota
	AlgoLouvain
)

type Community struct {
	ID       int
	Label    string
	Members  []string
	Cohesion float64
}

func DetectCommunities(ctx context.Context, db GraphAdapter, cfg IndexConfig, algo Algorithm) ([]Community, error) {
	adj, nodeNames, err := loadAdjacency(ctx, db, cfg.NodeLabels, cfg.EdgeLabels)
	if err != nil {
		return nil, fmt.Errorf("loading adjacency: %w", err)
	}
	if len(adj) == 0 {
		return nil, nil
	}

	var assignment map[string]int
	switch algo {
	case AlgoLouvain:
		assignment = Louvain(adj)
	default:
		assignment = labelPropagation(adj, 50)
	}

	byComm := make(map[int][]string)
	for uid, cid := range assignment {
		byComm[cid] = append(byComm[cid], uid)
	}

	type cidNodes struct {
		cid   int
		nodes []string
	}
	var sorted []cidNodes
	for cid, nodes := range byComm {
		sorted = append(sorted, cidNodes{cid, nodes})
	}
	sort.Slice(sorted, func(i, j int) bool { return len(sorted[i].nodes) > len(sorted[j].nodes) })

	var result []Community
	for newID, cn := range sorted {
		sort.Strings(cn.nodes)
		label := fmt.Sprintf("Community %d", newID)
		bestDeg := 0
		for _, uid := range cn.nodes {
			if deg := len(adj[uid]); deg > bestDeg {
				bestDeg = deg
				if name := nodeNames[uid]; name != "" {
					label = name
				}
			}
		}
		cohesion := ComputeCohesion(adj, cn.nodes)
		result = append(result, Community{
			ID:       newID,
			Label:    label,
			Members:  cn.nodes,
			Cohesion: cohesion,
		})

		for _, uid := range cn.nodes {
			nodeLabel := guessLabel(uid, cfg.NodeLabels)
			q := fmt.Sprintf("MATCH (n:`%s` {uid: $uid}) SET n.community = $cid", nodeLabel)
			_, _ = db.Execute(ctx, q, map[string]any{"uid": uid, "cid": int64(newID)})
		}
	}
	return result, nil
}

func GodNodes(ctx context.Context, db GraphAdapter, nodeLabels []string, topN int) ([]map[string]any, error) {
	if topN <= 0 {
		topN = 10
	}
	type nodeInfo struct {
		uid    string
		name   string
		label  string
		degree int
	}
	var all []nodeInfo
	for _, label := range nodeLabels {
		q := fmt.Sprintf(`MATCH (n:`+"`%s`"+`)
			OPTIONAL MATCH (n)-[r]-()
			WITH n, count(r) AS degree
			RETURN n.uid AS uid, n.name AS name, '%s' AS label, degree
			ORDER BY degree DESC LIMIT %d`, label, label, topN*2)
		res, err := db.Query(ctx, q, nil)
		if err != nil {
			continue
		}
		for _, r := range res.Records {
			uid, _ := r["uid"].(string)
			name, _ := r["name"].(string)
			deg := toInt64(r["degree"])
			if uid != "" && deg > 0 {
				all = append(all, nodeInfo{uid, name, label, int(deg)})
			}
		}
	}
	sort.Slice(all, func(i, j int) bool { return all[i].degree > all[j].degree })
	if len(all) > topN {
		all = all[:topN]
	}
	var result []map[string]any
	for _, n := range all {
		result = append(result, map[string]any{
			"id":     n.uid,
			"name":   n.name,
			"label":  n.label,
			"degree": n.degree,
		})
	}
	return result, nil
}

func GraphCounts(ctx context.Context, db GraphAdapter, nodeLabels, edgeLabels []string) (int, int) {
	nodes := 0
	for _, label := range nodeLabels {
		q := fmt.Sprintf("MATCH (n:`%s`) RETURN count(n) AS c", label)
		res, err := db.Query(ctx, q, nil)
		if err == nil && len(res.Records) > 0 {
			nodes += int(toInt64(res.Records[0]["c"]))
		}
	}
	edges := 0
	for _, el := range edgeLabels {
		q := fmt.Sprintf("MATCH ()-[r:%s]->() RETURN count(r) AS c", el)
		res, err := db.Query(ctx, q, nil)
		if err == nil && len(res.Records) > 0 {
			edges += int(toInt64(res.Records[0]["c"]))
		}
	}
	return nodes, edges
}

func loadAdjacency(ctx context.Context, db GraphAdapter, nodeLabels, edgeLabels []string) (map[string][]string, map[string]string, error) {
	adj := make(map[string][]string)
	names := make(map[string]string)
	for _, label := range nodeLabels {
		q := fmt.Sprintf("MATCH (n:`%s`) RETURN n.uid AS uid, n.name AS name", label)
		res, err := db.Query(ctx, q, nil)
		if err != nil {
			continue
		}
		for _, r := range res.Records {
			uid, _ := r["uid"].(string)
			name, _ := r["name"].(string)
			if uid != "" {
				adj[uid] = nil
				names[uid] = name
			}
		}
	}
	for _, el := range edgeLabels {
		q := fmt.Sprintf("MATCH (a)-[:%s]->(b) RETURN a.uid AS src, b.uid AS dst", el)
		res, err := db.Query(ctx, q, nil)
		if err != nil {
			continue
		}
		for _, r := range res.Records {
			src, _ := r["src"].(string)
			dst, _ := r["dst"].(string)
			if src != "" && dst != "" {
				adj[src] = append(adj[src], dst)
				adj[dst] = append(adj[dst], src)
			}
		}
	}
	return adj, names, nil
}

func guessLabel(uid string, labels []string) string {
	if len(labels) > 0 {
		return labels[0]
	}
	return "Node"
}

func ComputeCohesion(adj map[string][]string, members []string) float64 {
	n := len(members)
	if n <= 1 {
		return 1.0
	}
	set := make(map[string]bool, n)
	for _, uid := range members {
		set[uid] = true
	}
	internal := 0
	for _, uid := range members {
		for _, nb := range adj[uid] {
			if set[nb] && nb > uid {
				internal++
			}
		}
	}
	possible := n * (n - 1) / 2
	if possible == 0 {
		return 0.0
	}
	return math.Round(float64(internal)/float64(possible)*100) / 100
}

func toInt64(v any) int64 {
	switch vv := v.(type) {
	case int64:
		return vv
	case int:
		return int64(vv)
	case float64:
		return int64(vv)
	}
	return 0
}

func labelPropagation(adj map[string][]string, maxIter int) map[string]int {
	labels := make(map[string]int, len(adj))
	nodeIDs := make([]string, 0, len(adj))
	idx := 0
	for uid := range adj {
		labels[uid] = idx
		nodeIDs = append(nodeIDs, uid)
		idx++
	}
	rng := rand.New(rand.NewSource(42))
	for iter := 0; iter < maxIter; iter++ {
		changed := false
		rng.Shuffle(len(nodeIDs), func(i, j int) { nodeIDs[i], nodeIDs[j] = nodeIDs[j], nodeIDs[i] })
		for _, uid := range nodeIDs {
			nb := adj[uid]
			if len(nb) == 0 {
				continue
			}
			lc := make(map[int]int)
			for _, n := range nb {
				lc[labels[n]]++
			}
			best, bestCnt := labels[uid], 0
			for lbl, cnt := range lc {
				if cnt > bestCnt || (cnt == bestCnt && lbl < best) {
					best, bestCnt = lbl, cnt
				}
			}
			if best != labels[uid] {
				labels[uid] = best
				changed = true
			}
		}
		if !changed {
			break
		}
	}
	return labels
}

func Louvain(adj map[string][]string) map[string]int {
	nodeIdx := make(map[string]int)
	var nodes []string
	for uid := range adj {
		nodeIdx[uid] = len(nodes)
		nodes = append(nodes, uid)
	}
	n := len(nodes)
	if n == 0 {
		return map[string]int{}
	}
	type edge struct{ to int }
	adjInt := make([][]edge, n)
	degree := make([]float64, n)
	totalW := 0.0
	for uid, nb := range adj {
		i := nodeIdx[uid]
		seen := make(map[int]bool)
		for _, nuid := range nb {
			j, ok := nodeIdx[nuid]
			if !ok || j == i || seen[j] {
				continue
			}
			seen[j] = true
			adjInt[i] = append(adjInt[i], edge{j})
			degree[i]++
			totalW++
		}
	}
	if totalW == 0 {
		res := make(map[string]int, n)
		for i, uid := range nodes {
			res[uid] = i
		}
		return res
	}
	community := make([]int, n)
	for i := range community {
		community[i] = i
	}
	improved := true
	for pass := 0; improved && pass < 20; pass++ {
		improved = false
		for i := 0; i < n; i++ {
			cur := community[i]
			nc := make(map[int]float64)
			for _, e := range adjInt[i] {
				nc[community[e.to]]++
			}
			edgesToCur := nc[cur]
			bestComm, bestDelta := cur, 0.0
			cd := make(map[int]float64)
			for j := 0; j < n; j++ {
				cd[community[j]] += degree[j]
			}
			for comm, edgesToComm := range nc {
				if comm == cur {
					continue
				}
				delta := (edgesToComm-edgesToCur)/totalW -
					degree[i]*(cd[comm]-cd[cur]+degree[i])/(totalW*totalW)
				if delta > bestDelta {
					bestDelta = delta
					bestComm = comm
				}
			}
			if bestComm != cur {
				community[i] = bestComm
				improved = true
			}
		}
	}
	commMap := make(map[int]int)
	nextID := 0
	res := make(map[string]int, n)
	for i, uid := range nodes {
		cid := community[i]
		if _, ok := commMap[cid]; !ok {
			commMap[cid] = nextID
			nextID++
		}
		res[uid] = commMap[cid]
	}
	return res
}
