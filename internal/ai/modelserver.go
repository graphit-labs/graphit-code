package ai

import (
	"bufio"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"net"
	"os"
	"sync"
	"sync/atomic"
	"time"
)

// Local models run in model-server processes of their own — `graphit model serve
// embedding` and `graphit model serve rerank` — which the daemon starts and supervises.
// Every other process reaches a model through the server's socket, so each model's CPU
// and memory show up under one identifiable process, a crashing model cannot take its
// caller down, and a model is loaded once per machine instead of once per caller.

var modelServerRequired atomic.Bool

// SetModelServerRequired makes this process refuse to load a local model itself. The
// daemon sets it: a local model it needs must come from its model-server process, and
// while that server is unavailable the request fails instead of loading the model into
// the daemon.
func SetModelServerRequired(required bool) { modelServerRequired.Store(required) }

// modelServerWait bounds how long a process that requires the server waits for it to
// come up — the daemon starts it together with everything else.
var modelServerWait = 10 * time.Second

func setModelServerWaitForTest(d time.Duration) func() {
	old := modelServerWait
	modelServerWait = d
	return func() { modelServerWait = old }
}

// serviceRunsLocally reports whether task resolves to a local model. Without an active
// account profile the default provider is local.
func serviceRunsLocally(task ModelTask) bool {
	if _, ok := activeAuthSnapshot(); !ok {
		return true
	}
	return ConfiguredServiceIsLocal(task)
}

// errModelServerUnavailable reports a local model that this process may not load.
func errModelServerUnavailable(task ModelTask) error {
	return fmt.Errorf("local %s model server is not running (socket %s)", task, ModelSocketPath(task))
}

// dialModelServer reports whether the server for task accepts connections, waiting up
// to wait for it to appear.
func dialModelServer(task ModelTask, wait time.Duration) bool {
	deadline := time.Now().Add(wait)
	for {
		conn, err := net.DialTimeout("unix", ModelSocketPath(task), time.Second)
		if err == nil {
			_ = conn.Close()
			return true
		}
		if time.Now().After(deadline) {
			return false
		}
		time.Sleep(200 * time.Millisecond)
	}
}

// ── Server ──────────────────────────────────────────────────────────────────────────

// modelHandler answers one decoded request line with one response value.
type modelHandler func(ctx context.Context, line []byte) any

// ServeModel serves task on sockFile until ctx ends. The model itself is loaded on the
// first request, so an idle server costs no model memory.
func ServeModel(ctx context.Context, task ModelTask, sockFile string) error {
	var handle modelHandler
	switch task {
	case ModelTaskEmbedding:
		handle = embeddingHandler(NewLazyEmbeddingClient())
	case ModelTaskRerank:
		handle = rerankHandler(&lazyScorer{})
	default:
		return fmt.Errorf("no model server for task %q", task)
	}
	return serveModelSocket(ctx, sockFile, handle)
}

func serveModelSocket(ctx context.Context, sockFile string, handle modelHandler) error {
	if n := len(sockFile); n > maxSocketPathLen {
		return fmt.Errorf("model server: socket path %s is %d bytes, over the %d the operating system accepts", sockFile, n, maxSocketPathLen)
	}
	if err := prepareSocketDir(sockFile); err != nil {
		return fmt.Errorf("model server: %w", err)
	}
	// A previous server that died without cleaning up leaves its socket behind; a live
	// one is impossible here because the daemon runs one server per task.
	_ = os.Remove(sockFile)
	listener, err := net.Listen("unix", sockFile)
	if err != nil {
		return fmt.Errorf("model server listen %s: %w", sockFile, err)
	}
	go func() {
		<-ctx.Done()
		_ = listener.Close()
	}()
	defer func() { _ = os.Remove(sockFile) }()

	var conns sync.WaitGroup
	defer conns.Wait()
	for {
		conn, err := listener.Accept()
		if err != nil {
			if ctx.Err() != nil {
				return nil
			}
			return fmt.Errorf("model server accept: %w", err)
		}
		conns.Add(1)
		go func() {
			defer conns.Done()
			serveModelConn(ctx, conn, handle)
		}()
	}
}

func serveModelConn(ctx context.Context, conn net.Conn, handle modelHandler) {
	defer func() { _ = conn.Close() }()
	go func() {
		<-ctx.Done()
		_ = conn.Close()
	}()
	reader := bufio.NewReader(conn)
	for {
		line, err := reader.ReadBytes('\n')
		if err != nil {
			return
		}
		resp, _ := json.Marshal(handle(ctx, line))
		if _, err := conn.Write(append(resp, '\n')); err != nil {
			return
		}
	}
}

func embeddingHandler(client EmbeddingClient) modelHandler {
	return func(ctx context.Context, line []byte) any {
		var req embedRequest
		if err := json.Unmarshal(line, &req); err != nil {
			return embedResponse{Error: fmt.Sprintf("invalid json: %v", err)}
		}
		var vectors [][]float32
		var err error
		switch {
		case len(req.Texts) > 0:
			vectors, err = client.EmbedBatch(ctx, req.Texts)
		case req.Query != "":
			var vec []float32
			if qe, ok := client.(QueryEmbedder); ok {
				vec, err = qe.EmbedQuery(ctx, req.Query)
			} else {
				vec, err = client.Embed(ctx, req.Query)
			}
			vectors = [][]float32{vec}
		}
		if err != nil {
			return embedResponse{Error: err.Error()}
		}
		return embedResponse{Vectors: vectors}
	}
}

type rerankRequest struct {
	Query      string   `json:"query"`
	Candidates []string `json:"candidates"`
}

type rerankResponse struct {
	Scores []float64 `json:"scores,omitempty"`
	Model  string    `json:"model,omitempty"`
	Error  string    `json:"error,omitempty"`
}

func rerankHandler(scorer Scorer) modelHandler {
	return func(ctx context.Context, line []byte) any {
		var req rerankRequest
		if err := json.Unmarshal(line, &req); err != nil {
			return rerankResponse{Error: fmt.Sprintf("invalid json: %v", err)}
		}
		scores, err := scorer.Score(ctx, req.Query, req.Candidates)
		if err != nil {
			return rerankResponse{Error: err.Error()}
		}
		return rerankResponse{Scores: scores, Model: scorer.Name()}
	}
}

// lazyScorer builds the configured reranker on first use and rebuilds it when the
// active rerank configuration changes, remembering a failure until then so a broken
// install is not retried on every query.
type lazyScorer struct {
	mu     sync.Mutex
	key    string
	scorer Scorer
	err    error
}

func activeRerankConfigurationKey() string {
	execution, executionErr := configuredONNXExecution(ModelTaskRerank)
	identity := configuredModelID(ModelTaskRerank) + "\x00" + fmt.Sprint(execution) + "\x00" + fmt.Sprint(executionErr)
	if snapshot, ok := activeAuthSnapshot(); ok {
		return snapshot.Provider.Name + "\x00" + fmt.Sprint(snapshot.Provider.Revision) + "\x00" + snapshot.Profile.Name + "\x00" + identity
	}
	return "local-default\x00" + identity
}

func (l *lazyScorer) current(ctx context.Context) (Scorer, error) {
	key := activeRerankConfigurationKey()
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.key == key && (l.scorer != nil || l.err != nil) {
		return l.scorer, l.err
	}
	if closer, ok := l.scorer.(interface{ Close() error }); ok {
		_ = closer.Close()
	}
	adapter, err := NewRerankerFromConfig(ctx)
	l.key, l.scorer, l.err = key, nil, err
	if err == nil {
		l.scorer = adapter.Scorer
	}
	return l.scorer, l.err
}

func (l *lazyScorer) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	scorer, err := l.current(ctx)
	if err != nil {
		return nil, err
	}
	return scorer.Score(ctx, query, candidates)
}

func (l *lazyScorer) Name() string {
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.scorer != nil {
		return l.scorer.Name()
	}
	return "reranker (lazy, not loaded)"
}

// ── Rerank client ───────────────────────────────────────────────────────────────────

// proxyScorer scores through the rerank model server.
type proxyScorer struct {
	sockFile string
	name     atomic.Value
}

func newProxyScorer() *proxyScorer {
	s := &proxyScorer{sockFile: ModelSocketPath(ModelTaskRerank)}
	s.name.Store("rerank-server (" + proxyModelTag + ")")
	return s
}

func (s *proxyScorer) Name() string { return s.name.Load().(string) }

func (s *proxyScorer) Score(ctx context.Context, query string, candidates []string) ([]float64, error) {
	if len(candidates) == 0 {
		return nil, nil
	}
	var dialer net.Dialer
	conn, err := dialer.DialContext(ctx, "unix", s.sockFile)
	if err != nil {
		return nil, fmt.Errorf("rerank server dial: %w", err)
	}
	defer func() { _ = conn.Close() }()
	if deadline, ok := ctx.Deadline(); ok {
		_ = conn.SetDeadline(deadline)
	}
	stop := context.AfterFunc(ctx, func() { _ = conn.Close() })
	defer stop()

	req, err := json.Marshal(rerankRequest{Query: query, Candidates: candidates})
	if err != nil {
		return nil, err
	}
	if _, err := conn.Write(append(req, '\n')); err != nil {
		return nil, fmt.Errorf("rerank server write: %w", errors.Join(err, ctx.Err()))
	}
	line, err := bufio.NewReader(conn).ReadBytes('\n')
	if err != nil {
		return nil, fmt.Errorf("rerank server read: %w", errors.Join(err, ctx.Err()))
	}
	var resp rerankResponse
	if err := json.Unmarshal(line, &resp); err != nil {
		return nil, fmt.Errorf("rerank server decode: %w", err)
	}
	if resp.Error != "" {
		return nil, fmt.Errorf("rerank server: %s", resp.Error)
	}
	if resp.Model != "" {
		s.name.Store(resp.Model)
	}
	return resp.Scores, nil
}

// ConfiguredReranker resolves the scorer a search uses: the rerank model server
// when it is up, otherwise the configured reranker built here — unless this process
// must not load a local model, in which case the server's absence is an error.
func ConfiguredReranker(ctx context.Context) (*RerankAdapter, error) {
	local := serviceRunsLocally(ModelTaskRerank)
	wait := time.Duration(0)
	if local && modelServerRequired.Load() {
		wait = modelServerWait
	}
	if dialModelServer(ModelTaskRerank, wait) {
		return &RerankAdapter{Scorer: newProxyScorer()}, nil
	}
	if local && modelServerRequired.Load() {
		return nil, errModelServerUnavailable(ModelTaskRerank)
	}
	return NewRerankerFromConfig(ctx)
}

// LogModelServerStart records which model a server process is about to serve.
func LogModelServerStart(task ModelTask) {
	slog.Info("model server starting", "task", task, "socket", ModelSocketPath(task), "pid", os.Getpid())
}
