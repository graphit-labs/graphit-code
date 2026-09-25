package uiserver

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"os"
	"sort"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/knowledge"
	"github.com/graphit-labs/graphit-code/internal/store"
	"github.com/graphit-labs/graphit-code/internal/wiki"
)

type livePage struct {
	WikiPageContent
	Context    string `json:"context"`
	ArtifactID string `json:"artifact_id,omitempty"`
	Version    string `json:"version,omitempty"`
}

func openLiveKnowledge(ctx context.Context, workdir, name string) (*wiki.WikiDB, error) {
	rec, ok := store.LookupContext(workdir, store.KindKnowledge, name)
	if !ok {
		return nil, fmt.Errorf("knowledge context is not installed in this investigation")
	}
	if !rec.IsHub() {
		path := knowledge.ReadDirIn(workdir, name)
		if _, err := os.Stat(path); err != nil {
			return nil, fmt.Errorf("knowledge source is unavailable: %w", err)
		}
		return wiki.OpenWikiDB(ctx, path)
	}
	st, err := hub.NewS3Store(ctx, nil, config.LoadProjectConfig(workdir))
	if err != nil {
		return nil, err
	}
	if !st.Configured() {
		return nil, fmt.Errorf("hub storage is not configured")
	}
	mount, ok, err := st.MountedWikiAt(ctx, rec.ArtifactID, rec.Version, rec.ProjectID)
	if err != nil {
		return nil, err
	}
	if !ok {
		return nil, fmt.Errorf("published knowledge source is unavailable")
	}
	return wiki.OpenWikiDBAt(ctx, mount.Config)
}
func liveKnowledgePage(ctx context.Context, workdir, target, contextName string) ([]livePage, error) {
	contexts := knowledge.InstalledContextsIn(workdir)
	sort.Slice(contexts, func(i, j int) bool { return len(contexts[i]) > len(contexts[j]) })
	// A prefix is context syntax only when it names an installed context.
prefix:
	for _, name := range contexts {
		for _, separator := range []string{":", "/"} {
			if rest, ok := strings.CutPrefix(target, name+separator); ok {
				contextName = name
				target = rest
				break prefix
			}
		}
	}
	sort.Strings(contexts)
	target = strings.TrimSuffix(strings.TrimSpace(target), ".md")
	if target == "" {
		return nil, fmt.Errorf("page is required")
	}
	if contextName != "" {
		if _, ok := store.LookupContext(workdir, store.KindKnowledge, contextName); !ok {
			return nil, fmt.Errorf("knowledge context is not installed in this investigation")
		}
		contexts = []string{contextName}
	}
	matches := []livePage{}
	for _, name := range contexts {
		db, err := openLiveKnowledge(ctx, workdir, name)
		if err != nil {
			return nil, err
		}
		chunk, err := db.Chunk(ctx, target)
		db.Close()
		if errors.Is(err, wiki.ErrPageNotFound) {
			continue
		}
		if err != nil {
			return nil, err
		}
		if chunk == nil {
			continue
		}
		rec, _ := store.LookupContext(workdir, store.KindKnowledge, name)
		matches = append(matches, livePage{WikiPageContent: WikiPageContent{WikiPageMeta: chunkPageMeta(*chunk, nil), Content: chunk.Body}, Context: name, ArtifactID: rec.ArtifactID, Version: rec.Version})
	}
	return matches, nil
}
func (h *LiveHandler) handleKnowledgePage(w http.ResponseWriter, r *http.Request) {
	if !h.requireOwner(w, r) {
		return
	}
	dir, err := h.mgr.WorkspaceForRead(r.PathValue("id"))
	if err != nil {
		writeLiveError(w, err)
		return
	}
	pages, err := liveKnowledgePage(r.Context(), dir, r.URL.Query().Get("page"), r.URL.Query().Get("context"))
	if err != nil {
		writeJSONError(w, err)
		return
	}
	if len(pages) == 0 {
		http.Error(w, "Document is unavailable in this investigation", http.StatusNotFound)
		return
	}
	if len(pages) > 1 {
		writeJSON(w, map[string]any{"candidates": pages})
		return
	}
	writeJSON(w, pages[0])
}
func writeJSONError(w http.ResponseWriter, err error) {
	w.WriteHeader(http.StatusBadGateway)
	writeJSON(w, map[string]string{"error": err.Error()})
}
