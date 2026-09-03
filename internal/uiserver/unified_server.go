package uiserver

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"io/fs"
	"net"
	"net/http"
	"strconv"
	"time"

	"github.com/graphit-labs/graphit-code/internal/ast"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/hub"
	"github.com/graphit-labs/graphit-code/internal/livesearch"
	"github.com/graphit-labs/graphit-code/internal/livesearch/prep"
	"github.com/graphit-labs/graphit-code/internal/netutil"
	graphitui "github.com/graphit-labs/graphit-code/internal/ui"
)

type UnifiedServer struct {
	port           int
	host           string
	allowedOrigins []string
	projectName    string
	mux            *http.ServeMux
	live           *LiveHandler
	agentFeatures  bool
}

func NewUnifiedServer(
	hubSvc *hub.HubService,
	ide string,
	astDB ast.GraphDB,
	repoPath string,
	projectName string,
) (*UnifiedServer, error) {

	projectCfg := config.LoadProjectConfig(repoPath)
	host := config.ResolveUIHost(nil, projectCfg)
	allowedOrigins := config.ResolveUIAllowedOrigins(nil, projectCfg)

	port, err := netutil.FindFreePortOnHost(host, 8080)
	if err != nil {
		return nil, fmt.Errorf("no free port: %w", err)
	}

	mux := http.NewServeMux()

	hubSrv, err := hub.NewUIServerOnPort(hubSvc, ide, port)
	if err != nil {
		return nil, fmt.Errorf("hub handler init: %w", err)
	}
	astSrv, err := ast.NewServerOnPort(astDB, repoPath, port)
	if err != nil {
		return nil, fmt.Errorf("ast handler init: %w", err)
	}

	hubSrv.RegisterAPIRoutes(mux)
	astSrv.RegisterAPIRoutes(mux)

	// Wiki page browsing. The live search is a separate subsystem with its own
	// session runtime and SSE transport; see internal/livesearch.
	wikiHandler := NewWikiHandler(hubSvc)
	wikiHandler.RegisterAPIRoutes(mux)

	// Live search is the heaviest of the agent-dependent features and the only one whose routes
	// are not registered at all when the module is off: the others have a handler that refuses,
	// but a live session also prepares an ephemeral project and spawns a process, so there is
	// nothing worth keeping reachable. A request to /api/live/* then 404s, which is the honest
	// answer — the feature is not here.
	agentFeatures := config.AgentFeaturesEnabled(nil, projectCfg)
	var liveHandler *LiveHandler
	if agentFeatures {
		// Each session runs inside an ephemeral project that prep builds for it: a
		// lockfile, framework skills, lifecycle hooks, MCP servers, and the chosen
		// artifacts indexed and ready to query.
		liveMgr := livesearch.NewManagerFromConfig("", prep.Prepare)
		liveMgr.SetReclaim(prep.Reclaim)
		liveHandler = NewLiveHandler(liveMgr)
		liveHandler.RegisterAPIRoutes(mux)
	}

	daemonDreamHandler := NewDaemonDreamHandler(hubSvc)
	daemonDreamHandler.RegisterAPIRoutes(mux)

	s := &UnifiedServer{
		port: port, host: host, allowedOrigins: allowedOrigins,
		projectName: projectName, mux: mux, live: liveHandler,
		agentFeatures: agentFeatures,
	}

	mux.HandleFunc("/health", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = w.Write([]byte(`{"status":"ok"}`))
	})

	mux.HandleFunc("/ready", func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		w.WriteHeader(http.StatusOK)
		_, _ = fmt.Fprintf(w, `{"status":"ready","port":%d}`, s.port)
	})

	mux.HandleFunc("/", s.handleUI)

	return s, nil
}

func (s *UnifiedServer) Port() int { return s.port }

func (s *UnifiedServer) Host() string { return s.host }

func (s *UnifiedServer) Start(ctx context.Context) error {
	ln, _, err := netutil.ListenOnFreePortOnHost(s.host, s.port)
	if err != nil {
		return fmt.Errorf("listen: %w", err)
	}

	srv := &http.Server{
		Addr:    net.JoinHostPort(s.host, strconv.Itoa(s.port)),
		Handler: hub.CorsWrapWithAllowedOrigins(s.mux, s.allowedOrigins),
	}

	errCh := make(chan error, 1)
	go func() { errCh <- srv.Serve(ln) }()

	<-ctx.Done()
	shutCtx, cancel := context.WithTimeout(context.Background(), 5*time.Second)
	defer cancel()
	_ = srv.Shutdown(shutCtx)

	// After the listener, because a session outliving its stream is normal and only
	// the process going away ends one. This stops the agent processes and closes
	// each event log, which the sessions cannot do for themselves once we exit.
	if s.live != nil {
		s.live.Manager().CloseAll()
	}

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
	// __AGENT_FEATURES__ is injected rather than fetched because the UI must never OFFER a
	// feature it cannot deliver — not even for the one render before a capabilities request comes
	// back. A button that appears and then disappears is worse than one that was never there, and
	// a button that stays and fails is worse still.
	injection := fmt.Sprintf(`<script>
  window.__APP_MODE__ = "unified";
  window.__PROJECT_NAME__ = %q;
  window.__AGENT_FEATURES__ = %t;
</script>`, s.projectName, s.agentFeatures)
	data = bytes.Replace(data, []byte("</head>"), []byte(injection+"</head>"), 1)
	w.Header().Set("Content-Type", "text/html; charset=utf-8")
	_, _ = w.Write(data)
}
