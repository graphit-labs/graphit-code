package wiki

import (
	"math"
)

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
