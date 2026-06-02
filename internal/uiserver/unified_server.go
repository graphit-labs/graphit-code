package uiserver

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io/fs"
	"net/http"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/netutil"
	graphitui "github.com/graphit-labs/graphit-code/internal/ui"
)

type UnifiedServer struct {
	port        int
	projectName string
	mux         *http.ServeMux
}

func NewUnifiedServer(
	hubSvc *hub.HubService,
	ide string,
	astDB ast.GraphDB,
	astJobs *ast.JobManager,
	repoPath string,
	projectName string,
) (*UnifiedServer, error) {

	port, err := netutil.FindFreePort(8080)
	if err != nil {
		return nil, fmt.Errorf("no free port: %w", err)
	}

	mux := http.NewServeMux()

	hubSrv, err := hub.NewUIServerOnPort(hubSvc, ide, port)
	if err != nil {
		return nil, fmt.Errorf("hub handler init: %w", err)
	}
	astSrv, err := ast.NewServerOnPort(astDB, astJobs, repoPath, port)
	if err != nil {
		return nil, fmt.Errorf("ast handler init: %w", err)
	}

	hubSrv.RegisterAPIRoutes(mux)
	astSrv.RegisterAPIRoutes(mux)

	wikiHandler := NewWikiHandler(hubSvc)
	wikiHandler.RegisterAPIRoutes(mux)

	s := &UnifiedServer{port: port, projectName: projectName, mux: mux}

	mux.HandleFunc("/", s.handleUI)

	return s, nil
}

func (s *UnifiedServer) Port() int { return s.port }

func (s *UnifiedServer) Start(ctx context.Context) error {
	ln, _, err := netutil.ListenOnFreePort(s.port)
	if err != nil {
		return fmt.Errorf("listen: %w", err)
	}

	srv := &http.Server{
		Addr:    fmt.Sprintf(":%d", s.port),
		Handler: hub.CorsWrap(s.mux),
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

func (s *UnifiedServer) handleUI(w http.ResponseWriter, r *http.Request) {
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
  window.__APP_MODE__ = "unified";
  window.__PROJECT_NAME__ = %q;
</script>`, apiBase, s.projectName)
	data = bytes.Replace(data, []byte("</head>"), []byte(injection+"</head>"), 1)
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write(data)
}
