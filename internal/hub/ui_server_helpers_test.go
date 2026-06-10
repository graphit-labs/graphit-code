package hub

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io/fs"
	"net/http"
	"time"

	"github.com/graphit-labs/graphit-code/internal/netutil"
	graphitui "github.com/graphit-labs/graphit-code/internal/ui"
)

func NewUIServer(svc *HubService, ide string) (*UIServer, error) {
	port, err := netutil.FindFreePort(8100)
	if err != nil {
		return nil, fmt.Errorf("no free port: %w", err)
	}
	s := &UIServer{svc: svc, port: port, mux: http.NewServeMux(), ide: ide}
	s.registerRoutes()
	return s, nil
}

func (s *UIServer) Port() int { return s.port }

func (s *UIServer) Start(ctx context.Context) error {
	srv := &http.Server{
		Addr:    fmt.Sprintf(":%d", s.port),
		Handler: corsWrap(s.mux),
	}
	ln, _, err := netutil.ListenOnFreePort(s.port)
	if err != nil {
		return err
	}
	errCh := make(chan error, 1)
	go func() { errCh <- srv.Serve(ln) }()
	<-ctx.Done()
	shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutCtx)
	select {
	case e := <-errCh:
		if e != nil && !errors.Is(e, http.ErrServerClosed) {
			return e
		}
	default:
	}
	return nil
}

func (s *UIServer) registerRoutes() {
	s.RegisterAPIRoutes(s.mux)
	s.mux.HandleFunc("/", s.handleUI)
}

func (s *UIServer) handleUI(w http.ResponseWriter, r *http.Request) {
	if graphitui.ServeStatic(w, r) {
		return
	}
	data, err := fs.ReadFile(graphitui.DistFS, "dist/index.html")
	if err != nil {
		http.Error(w, "UI not found: "+err.Error(), 500)
		return
	}
	apiBase := fmt.Sprintf("http://localhost:%d", s.port)
	injection := fmt.Sprintf(`<script>
  window.__API_BASE__ = %q;
  window.__APP_MODE__ = "hub";
</script>`, apiBase)
	data = bytes.Replace(data, []byte("</head>"), []byte(injection+"</head>"), 1)
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write(data)
}

func corsWrap(h http.Handler) http.Handler { return CorsWrap(h) }
