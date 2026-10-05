package commands

import (
	"context"
	"encoding/json"
	"net/http"
	"strings"
	"time"
)

type daemonUIHandler struct {
	runtimeKey string
	ensure     func(context.Context) (string, error)
}

func (h *daemonUIHandler) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	token, ok := strings.CutPrefix(r.Header.Get("Authorization"), "Bearer ")
	if !ok || h.runtimeKey == "" || !secretEqual(token, h.runtimeKey) {
		http.Error(w, "unauthorized", http.StatusUnauthorized)
		return
	}
	if h.ensure == nil {
		http.Error(w, "daemon UI unavailable", http.StatusServiceUnavailable)
		return
	}
	ctx, cancel := context.WithTimeout(r.Context(), 30*time.Second)
	defer cancel()
	address, err := h.ensure(ctx)
	if err != nil {
		http.Error(w, "daemon UI did not become ready", http.StatusServiceUnavailable)
		return
	}
	w.Header().Set("Content-Type", "application/json")
	_ = json.NewEncoder(w).Encode(struct {
		URL string `json:"url"`
	}{address})
}
