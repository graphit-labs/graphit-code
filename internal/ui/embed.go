package ui

import (
	"embed"
	"io/fs"
	"mime"
	"net/http"
	"path"
	"strings"
)

//go:embed dist/*
var DistFS embed.FS

func ServeStatic(w http.ResponseWriter, r *http.Request) bool {
	if r.URL.Path == "/" || strings.HasPrefix(r.URL.Path, "/api/") {
		return false
	}
	fp := "dist" + r.URL.Path
	data, err := fs.ReadFile(DistFS, fp)
	if err != nil {
		return false
	}
	ct := mime.TypeByExtension(path.Ext(r.URL.Path))
	if ct == "" {
		ct = "application/octet-stream"
	}
	w.Header().Set("Content-Type", ct)
	w.Header().Set("Cache-Control", "public, max-age=3600")
	w.Write(data)
	return true
}
