package commands

import (
	"encoding/json"
	"net/http"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/sessionhook"
)

// sessionHookHandler applies the store-backed half of Agent hook events for the
// lightweight hook binary — the Task lifecycle effect and mandatory memory — so a hook
// fired on every tool call renders in that small process and costs the daemon one local
// request, never a core process opening the stores. Only the local runtime key is
// accepted: the caller is a process on this machine that can read the daemon's key
// file, exactly like the stdio MCP proxy.
type sessionHookHandler struct {
	runtimeKey string
}

func (h *sessionHookHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	token, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
	if !ok || h.runtimeKey == "" || !secretEqual(token, h.runtimeKey) {
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return
	}
	var req sessionhook.StoreRequest
	if err := json.NewDecoder(http.MaxBytesReader(w, r.Body, 8<<20)).Decode(&req); err != nil {
		writeSessionHookResponse(w, http.StatusBadRequest, sessionhook.StoreResponse{Error: "invalid session hook request"})
		return
	}
	// The caller resolved the project; accept only a real project root so the request
	// cannot point the daemon's stores at an arbitrary directory.
	if req.ProjectDir == "" || sessionhook.FindProjectRoot(req.ProjectDir) != req.ProjectDir {
		writeSessionHookResponse(w, http.StatusBadRequest, sessionhook.StoreResponse{Error: "project_dir is not a Graphit project root"})
		return
	}
	var resp sessionhook.StoreResponse
	result, err := applySessionHookStore(r.Context(), req)
	if err != nil {
		resp.Error = err.Error()
	} else {
		resp.StoreResult = result
	}
	writeSessionHookResponse(w, http.StatusOK, resp)
}

// writeSessionHookResponse answers in the JSON shape the hook binary reads, errors
// included, so a rejected event is reported rather than retried through the core.
func writeSessionHookResponse(w http.ResponseWriter, status int, resp sessionhook.StoreResponse) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(resp)
}
