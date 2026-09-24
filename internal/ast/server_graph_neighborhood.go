package ast

import (
	"context"
	"fmt"
	"net/http"
	"sort"
	"strconv"
	"strings"

	ladybug "github.com/graphit-labs/graphit-code/internal/ladybugstore"
)

type graphNeighborhoodRequest struct {
	anchorLabel, anchorIdentity   string
	relationshipType, targetLabel string
	direction, cursor             string
	limit                         int
}

type graphNeighborhoodMember struct {
	from, table, to string
}

type graphNeighborhoodRow struct {
	record QueryRecord
	uid    string
}

func (s *Server) handleGraphNeighborhood(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	req := graphNeighborhoodRequest{
		anchorLabel: q.Get("anchor_label"), anchorIdentity: q.Get("anchor_identity"),
		relationshipType: q.Get("relationship_type"), targetLabel: q.Get("target_label"),
		direction: q.Get("direction"), cursor: q.Get("cursor"), limit: 100,
	}
	if raw := q.Get("limit"); raw != "" {
		limit, err := strconv.Atoi(raw)
		if err != nil || limit < 1 || limit > 100 {
			writeError(w, http.StatusBadRequest, "limit must be between 1 and 100")
			return
		}
		req.limit = limit
	}
	if !isIdentifier(req.anchorLabel) || !isIdentifier(req.targetLabel) ||
		!isIdentifier(req.relationshipType) || req.anchorIdentity == "" ||
		(req.direction != "incoming" && req.direction != "outgoing") {
		writeError(w, http.StatusBadRequest, "a typed relationship, direction and indexed anchor identity are required")
		return
	}
	db := s.dbForContext(r)
	rows, next, err := queryGraphNeighborhood(r.Context(), db, req)
	if err != nil {
		writeError(w, http.StatusBadRequest, err.Error())
		return
	}
	nodes := map[string]map[string]any{}
	links := make([]map[string]any, 0, len(rows))
	for _, row := range rows {
		extractUserQueryGraph(row.record, nodes, &links)
	}
	normalizeGraphEdgeTypes(db, links)
	generation := ""
	if provider, ok := db.(canonicalManifestProvider); ok {
		if manifest, ok := provider.canonicalManifestSnapshot(); ok {
			generation = manifest.Generation
		}
	}
	ordered := make([]map[string]any, 0, len(nodes))
	ids := make([]string, 0, len(nodes))
	for id := range nodes {
		ids = append(ids, id)
	}
	sort.Strings(ids)
	for _, id := range ids {
		ordered = append(ordered, nodes[id])
	}
	writeJSON(w, map[string]any{"nodes": ordered, "links": links, "next_cursor": next, "index_generation": generation})
}

func queryGraphNeighborhood(ctx context.Context, db GraphDB, req graphNeighborhoodRequest) ([]graphNeighborhoodRow, string, error) {
	from, to := req.anchorLabel, req.targetLabel
	anchorVariable := "a"
	if req.direction == "incoming" {
		from, to = to, from
		anchorVariable = "b"
	}
	identityProperty := "uid"
	if req.anchorLabel == "File" || req.anchorLabel == "Directory" {
		identityProperty = "path"
	}
	members := []graphNeighborhoodMember{{from: from, table: req.relationshipType, to: to}}
	if provider, ok := db.(canonicalManifestProvider); ok {
		if manifest, ok := provider.canonicalManifestSnapshot(); ok {
			if !manifest.RelationUIDs {
				return nil, "", fmt.Errorf("this index has no stable relationship UIDs; reindex before exploring relationships")
			}
			identityProperty = canonicalPKFor(manifest, req.anchorLabel)
			members = nil
			for _, group := range manifest.RelGroups {
				if !strings.EqualFold(group.Type, req.relationshipType) {
					continue
				}
				for _, member := range group.Members {
					if member.From == from && member.To == to && member.Rows > 0 {
						members = append(members, graphNeighborhoodMember{from, member.Table, to})
					}
				}
			}
		}
	}
	if !isIdentifier(identityProperty) {
		return nil, "", fmt.Errorf("unsupported anchor identity property")
	}
	if len(members) == 0 {
		if err := ensureGraphAnchorExists(ctx, db, req, identityProperty); err != nil {
			return nil, "", err
		}
		return nil, "", fmt.Errorf("the requested relationship endpoint is not present in this index")
	}
	sort.Slice(members, func(i, j int) bool { return members[i].table < members[j].table })
	if req.cursor != "" && !strings.HasPrefix(req.cursor, "rel:") {
		return nil, "", fmt.Errorf("invalid relationship cursor")
	}
	rows := make([]graphNeighborhoodRow, 0, req.limit+1)
	for _, member := range members {
		if err := ctx.Err(); err != nil {
			return nil, "", err
		}
		query := fmt.Sprintf(
			"MATCH (a:%s)-[r:%s]->(b:%s) WHERE %s.%s = '%s' AND r.uid > '%s' RETURN a,r,b ORDER BY r.uid LIMIT %d",
			ladybug.QuoteIdent(member.from), ladybug.QuoteIdent(member.table), ladybug.QuoteIdent(member.to),
			anchorVariable, identityProperty, ladybug.EscapeLiteral(req.anchorIdentity),
			ladybug.EscapeLiteral(req.cursor), req.limit+1,
		)
		result, err := db.Query(ctx, query, nil)
		if err != nil {
			return nil, "", fmt.Errorf("relationship member %s: %w", member.table, err)
		}
		for _, record := range result.Records {
			rel, ok := record["r"].(map[string]any)
			if !ok {
				return nil, "", fmt.Errorf("relationship member %s returned no relation", member.table)
			}
			props, ok := rel["Properties"].(map[string]any)
			if !ok || safeStr(props["uid"]) == "" {
				return nil, "", fmt.Errorf("relationship member %s has no stable UID; reindex before exploring", member.table)
			}
			uid := safeStr(props["uid"])
			rows = append(rows, graphNeighborhoodRow{record: record, uid: uid})
		}
	}
	if len(rows) == 0 {
		if err := ensureGraphAnchorExists(ctx, db, req, identityProperty); err != nil {
			return nil, "", err
		}
	}
	sort.Slice(rows, func(i, j int) bool { return rows[i].uid < rows[j].uid })
	if len(rows) <= req.limit {
		return rows, "", nil
	}
	last := rows[req.limit-1]
	return rows[:req.limit], last.uid, nil
}

func ensureGraphAnchorExists(ctx context.Context, db GraphDB, req graphNeighborhoodRequest, identityProperty string) error {
	query := fmt.Sprintf("MATCH (a:%s) WHERE a.%s IN ['%s'] RETURN a.%s AS identity LIMIT 1",
		ladybug.QuoteIdent(req.anchorLabel), identityProperty, ladybug.EscapeLiteral(req.anchorIdentity), identityProperty)
	anchor, err := db.Query(ctx, query, nil)
	if err != nil {
		return fmt.Errorf("resolve selected entity: %w", err)
	}
	if len(anchor.Records) == 0 {
		return fmt.Errorf("selected entity no longer exists in the current index; refresh the result")
	}
	return nil
}
