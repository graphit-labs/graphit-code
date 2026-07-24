package ast

import (
	"sync"
	"testing"

	sitter "github.com/tree-sitter/go-tree-sitter"

	"github.com/graphit-labs/graphit-code/internal/ast/antlr/plsql"
	"github.com/graphit-labs/graphit-code/internal/ast/antlr/postgresql"
)

// ---------------------------------------------------------------------------
// Fixture: Go source (~200 lines)
// ---------------------------------------------------------------------------
const goSource = `package server

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"
	"sync"
	"time"
)

// Config holds the application configuration.
type Config struct {
	Host         string
	Port         int
	ReadTimeout  time.Duration
	WriteTimeout time.Duration
	MaxBodySize  int64
	Debug        bool
}

// DefaultConfig returns a Config with sane defaults.
func DefaultConfig() Config {
	return Config{
		Host:         "0.0.0.0",
		Port:         8080,
		ReadTimeout:  30 * time.Second,
		WriteTimeout: 30 * time.Second,
		MaxBodySize:  1 << 20,
		Debug:        false,
	}
}

// Logger defines the logging interface.
type Logger interface {
	Info(msg string, args ...any)
	Error(msg string, args ...any)
	Debug(msg string, args ...any)
	WithField(key string, value any) Logger
}

// stdLogger implements Logger using the standard library.
type stdLogger struct {
	logger *log.Logger
	fields map[string]any
}

func newStdLogger() *stdLogger {
	return &stdLogger{
		logger: log.New(os.Stdout, "[server] ", log.LstdFlags),
		fields: make(map[string]any),
	}
}

func (l *stdLogger) Info(msg string, args ...any)  { l.logger.Printf("INFO  "+msg, args...) }
func (l *stdLogger) Error(msg string, args ...any) { l.logger.Printf("ERROR "+msg, args...) }
func (l *stdLogger) Debug(msg string, args ...any) { l.logger.Printf("DEBUG "+msg, args...) }
func (l *stdLogger) WithField(key string, value any) Logger {
	n := &stdLogger{logger: l.logger, fields: make(map[string]any)}
	for k, v := range l.fields {
		n.fields[k] = v
	}
	n.fields[key] = value
	return n
}

// Server is the main application server.
type Server struct {
	cfg    Config
	mux    *http.ServeMux
	logger Logger
	pool   sync.Pool
	mu     sync.RWMutex
	routes map[string]http.HandlerFunc
}

// NewServer creates a new Server.
func NewServer(cfg Config, logger Logger) *Server {
	s := &Server{
		cfg:    cfg,
		mux:    http.NewServeMux(),
		logger: logger,
		routes: make(map[string]http.HandlerFunc),
	}
	s.pool.New = func() any {
		return make([]byte, 0, 4096)
	}
	return s
}

// RegisterRoute adds a handler for a path.
func (s *Server) RegisterRoute(path string, handler http.HandlerFunc) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.routes[path] = handler
	s.mux.HandleFunc(path, handler)
	s.logger.Info("registered route: %s", path)
}

// Start begins serving HTTP requests.
func (s *Server) Start(ctx context.Context) error {
	addr := fmt.Sprintf("%s:%d", s.cfg.Host, s.cfg.Port)
	srv := &http.Server{
		Addr:         addr,
		Handler:      s.mux,
		ReadTimeout:  s.cfg.ReadTimeout,
		WriteTimeout: s.cfg.WriteTimeout,
	}

	errCh := make(chan error, 1)
	go func() {
		s.logger.Info("starting server on %s", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			errCh <- err
		}
		close(errCh)
	}()

	select {
	case <-ctx.Done():
		shutdownCtx, cancel := context.WithTimeout(context.Background(), 10*time.Second)
		defer cancel()
		return srv.Shutdown(shutdownCtx)
	case err := <-errCh:
		return err
	}
}

// Response is a JSON response wrapper.
type Response struct {
	Status  int    ` + "`" + `json:"status"` + "`" + `
	Message string ` + "`" + `json:"message"` + "`" + `
	Data    any    ` + "`" + `json:"data,omitempty"` + "`" + `
}

// WriteJSON writes a JSON response.
func WriteJSON(w http.ResponseWriter, status int, data any) error {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	return json.NewEncoder(w).Encode(data)
}

// ReadJSON reads and decodes a JSON request body.
func ReadJSON(r *http.Request, v any) error {
	defer func() { _ = r.Body.Close() }()
	body, err := io.ReadAll(io.LimitReader(r.Body, 1<<20))
	if err != nil {
		return fmt.Errorf("read body: %w", err)
	}
	return json.Unmarshal(body, v)
}

// Middleware is a function that wraps an http.Handler.
type Middleware func(http.Handler) http.Handler

// Chain applies a sequence of middleware to a handler.
func Chain(h http.Handler, mw ...Middleware) http.Handler {
	for i := len(mw) - 1; i >= 0; i-- {
		h = mw[i](h)
	}
	return h
}

// LoggingMiddleware logs every request.
func LoggingMiddleware(logger Logger) Middleware {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			start := time.Now()
			next.ServeHTTP(w, r)
			logger.Info("%s %s took %v", r.Method, r.URL.Path, time.Since(start))
		})
	}
}

// RecoveryMiddleware catches panics.
func RecoveryMiddleware(logger Logger) Middleware {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			defer func() {
				if err := recover(); err != nil {
					logger.Error("panic recovered: %v", err)
					http.Error(w, "Internal Server Error", http.StatusInternalServerError)
				}
			}()
			next.ServeHTTP(w, r)
		})
	}
}

var _ Logger = (*stdLogger)(nil)
`

// ---------------------------------------------------------------------------
// Fixture: Python source (~200 lines)
// ---------------------------------------------------------------------------
const pythonSource = `"""
Data processing pipeline with multiple stages.
"""

import os
import sys
import json
import logging
from typing import Any, Dict, List, Optional, Tuple
from dataclasses import dataclass, field
from abc import ABC, abstractmethod
from contextlib import contextmanager
from pathlib import Path

logger = logging.getLogger(__name__)


@dataclass
class PipelineConfig:
    """Configuration for the processing pipeline."""
    name: str
    workers: int = 4
    batch_size: int = 100
    timeout: float = 30.0
    retry_count: int = 3
    output_dir: str = "/tmp/pipeline"
    debug: bool = False
    tags: List[str] = field(default_factory=list)
    metadata: Dict[str, Any] = field(default_factory=dict)


class PipelineError(Exception):
    """Base exception for pipeline errors."""
    def __init__(self, message: str, stage: str = "", cause: Optional[Exception] = None):
        super().__init__(message)
        self.stage = stage
        self.cause = cause


class Stage(ABC):
    """Abstract base class for pipeline stages."""

    def __init__(self, name: str, config: Optional[Dict] = None):
        self.name = name
        self.config = config or {}
        self._metrics: Dict[str, float] = {}

    @abstractmethod
    def process(self, data: Any) -> Any:
        """Process input data and return output."""
        ...

    @abstractmethod
    def validate(self, data: Any) -> bool:
        """Validate input data before processing."""
        ...

    def setup(self) -> None:
        """Optional setup hook called before processing."""
        pass

    def teardown(self) -> None:
        """Optional teardown hook called after processing."""
        pass

    def record_metric(self, name: str, value: float) -> None:
        self._metrics[name] = value

    @property
    def metrics(self) -> Dict[str, float]:
        return dict(self._metrics)


class TransformStage(Stage):
    """Stage that applies a transformation function."""

    def __init__(self, name: str, transform_fn, config=None):
        super().__init__(name, config)
        self.transform_fn = transform_fn

    def process(self, data: Any) -> Any:
        try:
            result = self.transform_fn(data)
            self.record_metric("items_processed", 1)
            return result
        except Exception as e:
            raise PipelineError(f"Transform failed: {e}", stage=self.name, cause=e)

    def validate(self, data: Any) -> bool:
        return data is not None


class FilterStage(Stage):
    """Stage that filters data based on a predicate."""

    def __init__(self, name: str, predicate, config=None):
        super().__init__(name, config)
        self.predicate = predicate

    def process(self, data: Any) -> Any:
        if isinstance(data, list):
            filtered = [item for item in data if self.predicate(item)]
            self.record_metric("items_filtered", len(data) - len(filtered))
            return filtered
        return data if self.predicate(data) else None

    def validate(self, data: Any) -> bool:
        return True


class Pipeline:
    """Orchestrates a sequence of processing stages."""

    def __init__(self, config: PipelineConfig):
        self.config = config
        self.stages: List[Stage] = []
        self._results: List[Tuple[str, Any]] = []

    def add_stage(self, stage: Stage) -> "Pipeline":
        self.stages.append(stage)
        return self

    @contextmanager
    def _stage_context(self, stage: Stage):
        stage.setup()
        try:
            yield stage
        finally:
            stage.teardown()

    def run(self, initial_data: Any) -> Any:
        data = initial_data
        for stage in self.stages:
            with self._stage_context(stage) as s:
                if not s.validate(data):
                    raise PipelineError(
                        f"Validation failed at stage {s.name}",
                        stage=s.name,
                    )
                data = s.process(data)
                self._results.append((s.name, data))
                if self.config.debug:
                    logger.debug(f"Stage {s.name}: {type(data).__name__}")
        return data

    def get_metrics(self) -> Dict[str, Dict[str, float]]:
        return {stage.name: stage.metrics for stage in self.stages}

    @classmethod
    def from_config(cls, config_path: str) -> "Pipeline":
        path = Path(config_path)
        with open(path) as f:
            raw = json.load(f)
        cfg = PipelineConfig(**raw)
        return cls(cfg)

    def __repr__(self) -> str:
        stages = " -> ".join(s.name for s in self.stages)
        return f"Pipeline({self.config.name}: {stages})"


def create_default_pipeline(name: str = "default") -> Pipeline:
    """Factory function for a standard pipeline."""
    config = PipelineConfig(name=name, debug=True)
    pipeline = Pipeline(config)
    pipeline.add_stage(TransformStage("normalize", lambda x: x))
    pipeline.add_stage(FilterStage("filter_empty", lambda x: x is not None))
    return pipeline


def load_data(source: str) -> List[Dict]:
    """Load data from a JSON file."""
    with open(source) as f:
        return json.load(f)


def save_results(data: Any, output_path: str) -> None:
    """Save processed results to a JSON file."""
    os.makedirs(os.path.dirname(output_path), exist_ok=True)
    with open(output_path, "w") as f:
        json.dump(data, f, indent=2)


if __name__ == "__main__":
    pipe = create_default_pipeline()
    if len(sys.argv) > 1:
        source_data = load_data(sys.argv[1])
        result = pipe.run(source_data)
        save_results(result, pipe.config.output_dir + "/output.json")
        print(f"Processed {len(source_data)} items")
    else:
        print("Usage: pipeline.py <input.json>")
`

// ---------------------------------------------------------------------------
// Fixture: JavaScript source (~200 lines)
// ---------------------------------------------------------------------------
const jsSource = `/**
 * Event-driven message bus with middleware support.
 * @module MessageBus
 */

const DEFAULT_OPTIONS = {
  maxListeners: 100,
  timeout: 5000,
  retryAttempts: 3,
  debug: false,
};

class EventEmitter {
  constructor() {
    this._handlers = new Map();
    this._onceHandlers = new Map();
  }

  on(event, handler) {
    if (!this._handlers.has(event)) {
      this._handlers.set(event, []);
    }
    this._handlers.get(event).push(handler);
    return this;
  }

  once(event, handler) {
    if (!this._onceHandlers.has(event)) {
      this._onceHandlers.set(event, []);
    }
    this._onceHandlers.get(event).push(handler);
    return this;
  }

  off(event, handler) {
    const handlers = this._handlers.get(event);
    if (handlers) {
      const idx = handlers.indexOf(handler);
      if (idx !== -1) handlers.splice(idx, 1);
    }
    return this;
  }

  emit(event, ...args) {
    const handlers = this._handlers.get(event) || [];
    const onceHandlers = this._onceHandlers.get(event) || [];
    handlers.forEach((h) => h(...args));
    onceHandlers.forEach((h) => h(...args));
    this._onceHandlers.delete(event);
    return handlers.length + onceHandlers.length > 0;
  }
}

class Middleware {
  constructor(name, fn) {
    this.name = name;
    this.fn = fn;
    this.enabled = true;
  }

  async execute(ctx, next) {
    if (!this.enabled) return next();
    return this.fn(ctx, next);
  }

  disable() { this.enabled = false; }
  enable() { this.enabled = true; }
}

class MessageBus extends EventEmitter {
  constructor(options = {}) {
    super();
    this.options = { ...DEFAULT_OPTIONS, ...options };
    this._middlewares = [];
    this._subscriptions = new Map();
    this._messageCount = 0;
  }

  use(name, fn) {
    this._middlewares.push(new Middleware(name, fn));
    return this;
  }

  subscribe(topic, handler) {
    const id = Symbol(topic);
    if (!this._subscriptions.has(topic)) {
      this._subscriptions.set(topic, new Map());
    }
    this._subscriptions.get(topic).set(id, handler);
    return { unsubscribe: () => this._subscriptions.get(topic)?.delete(id) };
  }

  async publish(topic, message) {
    this._messageCount++;
    const ctx = {
      topic,
      message,
      timestamp: Date.now(),
      id: this._messageCount,
      metadata: {},
    };

    const chain = this._buildMiddlewareChain(ctx, async () => {
      const handlers = this._subscriptions.get(topic);
      if (!handlers || handlers.size === 0) {
        this.emit("unhandled", ctx);
        return;
      }
      const promises = [];
      for (const [, handler] of handlers) {
        promises.push(this._executeWithTimeout(handler, ctx));
      }
      await Promise.allSettled(promises);
      this.emit("delivered", ctx);
    });

    try {
      await chain();
    } catch (error) {
      this.emit("error", { ...ctx, error });
      throw error;
    }
  }

  _buildMiddlewareChain(ctx, final) {
    let idx = 0;
    const next = async () => {
      if (idx < this._middlewares.length) {
        const mw = this._middlewares[idx++];
        return mw.execute(ctx, next);
      }
      return final();
    };
    return next;
  }

  async _executeWithTimeout(handler, ctx) {
    return new Promise((resolve, reject) => {
      const timer = setTimeout(
        () => reject(new Error("Handler timeout")),
        this.options.timeout
      );
      try {
        const result = handler(ctx);
        if (result && typeof result.then === "function") {
          result.then(resolve).catch(reject).finally(() => clearTimeout(timer));
        } else {
          clearTimeout(timer);
          resolve(result);
        }
      } catch (err) {
        clearTimeout(timer);
        reject(err);
      }
    });
  }

  getStats() {
    const topics = {};
    for (const [topic, handlers] of this._subscriptions) {
      topics[topic] = handlers.size;
    }
    return {
      messageCount: this._messageCount,
      middlewareCount: this._middlewares.length,
      topics,
    };
  }
}

const createLogger = (prefix = "bus") => ({
  info: (msg, ...args) => console.log("[" + prefix + "] " + msg, ...args),
  error: (msg, ...args) => console.error("[" + prefix + "] " + msg, ...args),
  debug: (msg, ...args) => console.debug("[" + prefix + "] " + msg, ...args),
});

const withRetry = (fn, attempts = 3) => async (...args) => {
  for (let i = 0; i < attempts; i++) {
    try { return await fn(...args); }
    catch (err) { if (i === attempts - 1) throw err; }
  }
};

export { MessageBus, EventEmitter, Middleware, createLogger, withRetry };
export default MessageBus;
`

// ---------------------------------------------------------------------------
// Fixture: PL/SQL source (~100 lines)
// ---------------------------------------------------------------------------
const plsqlSource = `CREATE OR REPLACE PROCEDURE process_employee_records(
    p_department_id IN NUMBER,
    p_effective_date IN DATE DEFAULT SYSDATE,
    p_status OUT VARCHAR2
) IS
    CURSOR c_employees IS
        SELECT employee_id, first_name, last_name, salary, hire_date
        FROM employees
        WHERE department_id = p_department_id
          AND status = 'ACTIVE'
        ORDER BY hire_date;

    TYPE t_emp_tab IS TABLE OF c_employees%ROWTYPE INDEX BY PLS_INTEGER;
    l_employees t_emp_tab;
    l_total_salary NUMBER := 0;
    l_avg_salary   NUMBER;
    l_emp_count    NUMBER := 0;
    l_bonus        NUMBER;
    l_max_records  CONSTANT NUMBER := 1000;

    e_too_many_records EXCEPTION;
    PRAGMA EXCEPTION_INIT(e_too_many_records, -20001);
BEGIN
    OPEN c_employees;
    FETCH c_employees BULK COLLECT INTO l_employees LIMIT l_max_records;
    CLOSE c_employees;

    IF l_employees.COUNT = 0 THEN
        p_status := 'NO_DATA';
        RETURN;
    ELSIF l_employees.COUNT >= l_max_records THEN
        RAISE e_too_many_records;
    END IF;

    FOR i IN 1..l_employees.COUNT LOOP
        l_total_salary := l_total_salary + l_employees(i).salary;
        l_emp_count := l_emp_count + 1;

        IF l_employees(i).salary > 100000 THEN
            l_bonus := l_employees(i).salary * 0.05;
        ELSIF l_employees(i).salary > 50000 THEN
            l_bonus := l_employees(i).salary * 0.10;
        ELSE
            l_bonus := l_employees(i).salary * 0.15;
        END IF;

        UPDATE employees
        SET bonus = l_bonus,
            last_review_date = p_effective_date,
            modified_by = USER,
            modified_date = SYSDATE
        WHERE employee_id = l_employees(i).employee_id;

        IF MOD(i, 500) = 0 THEN
            COMMIT;
        END IF;
    END LOOP;

    l_avg_salary := l_total_salary / NULLIF(l_emp_count, 0);

    INSERT INTO department_summary (
        department_id, total_salary, avg_salary,
        employee_count, report_date
    ) VALUES (
        p_department_id, l_total_salary, l_avg_salary,
        l_emp_count, SYSDATE
    );

    COMMIT;
    p_status := 'SUCCESS';

EXCEPTION
    WHEN e_too_many_records THEN
        ROLLBACK;
        p_status := 'TOO_MANY_RECORDS';
        INSERT INTO error_log (error_date, error_msg, procedure_name)
        VALUES (SYSDATE, 'Exceeded max records', 'process_employee_records');
        COMMIT;
    WHEN NO_DATA_FOUND THEN
        ROLLBACK;
        p_status := 'NO_DATA_FOUND';
    WHEN OTHERS THEN
        ROLLBACK;
        p_status := 'ERROR: ' || SQLERRM;
        INSERT INTO error_log (error_date, error_msg, procedure_name)
        VALUES (SYSDATE, SQLERRM, 'process_employee_records');
        COMMIT;
END process_employee_records;
/
`

// ---------------------------------------------------------------------------
// Fixture: PostgreSQL source (~80 lines)
// ---------------------------------------------------------------------------
const pgSource = `CREATE TABLE IF NOT EXISTS orders (
    order_id     SERIAL PRIMARY KEY,
    customer_id  INTEGER NOT NULL REFERENCES customers(customer_id),
    order_date   TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    status       VARCHAR(20) NOT NULL DEFAULT 'pending',
    total_amount NUMERIC(12,2) NOT NULL DEFAULT 0.00,
    notes        TEXT,
    created_at   TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at   TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT chk_status CHECK (status IN ('pending','confirmed','shipped','delivered','cancelled')),
    CONSTRAINT chk_amount CHECK (total_amount >= 0)
);

CREATE INDEX idx_orders_customer ON orders(customer_id);
CREATE INDEX idx_orders_status ON orders(status) WHERE status != 'cancelled';
CREATE INDEX idx_orders_date ON orders(order_date DESC);

CREATE OR REPLACE FUNCTION calculate_order_summary(
    p_customer_id INTEGER,
    p_start_date  TIMESTAMP WITH TIME ZONE DEFAULT NOW() - INTERVAL '30 days',
    p_end_date    TIMESTAMP WITH TIME ZONE DEFAULT NOW()
)
RETURNS TABLE (
    total_orders   BIGINT,
    total_spent    NUMERIC(12,2),
    avg_order      NUMERIC(12,2),
    max_order      NUMERIC(12,2),
    status_summary JSONB
) AS $$
BEGIN
    RETURN QUERY
    WITH order_stats AS (
        SELECT
            COUNT(*)           AS cnt,
            COALESCE(SUM(o.total_amount), 0) AS total,
            COALESCE(AVG(o.total_amount), 0) AS avg_amt,
            COALESCE(MAX(o.total_amount), 0) AS max_amt
        FROM orders o
        WHERE o.customer_id = p_customer_id
          AND o.order_date BETWEEN p_start_date AND p_end_date
          AND o.status != 'cancelled'
    ),
    status_agg AS (
        SELECT jsonb_object_agg(o.status, o.cnt) AS summary
        FROM (
            SELECT status, COUNT(*) AS cnt
            FROM orders
            WHERE customer_id = p_customer_id
              AND order_date BETWEEN p_start_date AND p_end_date
            GROUP BY status
        ) o
    )
    SELECT
        os.cnt,
        os.total,
        os.avg_amt,
        os.max_amt,
        COALESCE(sa.summary, '{}'::jsonb)
    FROM order_stats os
    CROSS JOIN status_agg sa;
END;
$$ LANGUAGE plpgsql STABLE;

SELECT
    c.customer_id,
    c.name,
    c.email,
    s.total_orders,
    s.total_spent,
    s.avg_order,
    s.status_summary
FROM customers c
CROSS JOIN LATERAL calculate_order_summary(c.customer_id) s
WHERE s.total_orders > 0
ORDER BY s.total_spent DESC
LIMIT 50;
`

// ---------------------------------------------------------------------------
// Tree-sitter query patterns for Go (from queries/go.yaml)
// ---------------------------------------------------------------------------
const goQueryPattern = `
(function_declaration name: (identifier) @name)
(method_declaration name: (field_identifier) @name)
(type_declaration (type_spec name: (type_identifier) @name type: (struct_type)))
(type_declaration (type_spec name: (type_identifier) @name type: (interface_type)))
(type_declaration (type_spec name: (type_identifier) @name))
(const_spec name: (identifier) @name)
(var_spec name: (identifier) @name)
(import_spec path: (interpreted_string_literal) @name)
(call_expression function: (identifier) @name)
(call_expression function: (selector_expression field: (field_identifier) @name))
`

// ===========================================================================
// Tree-sitter Benchmarks
// ===========================================================================

// BenchmarkTS_Native_FullParse_Go parses a ~200-line Go file from scratch
// (parser creation + language set + parse) on every iteration.
func BenchmarkTS_Native_FullParse_Go(b *testing.B) {
	src := []byte(goSource)
	lang := NativeLanguage("go")
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p := sitter.NewParser()
		_ = p.SetLanguage(lang)
		tree, err := tsParse(p, src)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Native_FullParse_Python parses a ~200-line Python file.
func BenchmarkTS_Native_FullParse_Python(b *testing.B) {
	src := []byte(pythonSource)
	lang := NativeLanguage("python")
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p := sitter.NewParser()
		_ = p.SetLanguage(lang)
		tree, err := tsParse(p, src)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Native_FullParse_JavaScript parses a ~200-line JavaScript file.
func BenchmarkTS_Native_FullParse_JavaScript(b *testing.B) {
	src := []byte(jsSource)
	lang := NativeLanguage("javascript")
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p := sitter.NewParser()
		_ = p.SetLanguage(lang)
		tree, err := tsParse(p, src)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Native_ParseReuse_Go measures parse-only performance with a
// pre-created parser (no parser creation overhead).
func BenchmarkTS_Native_ParseReuse_Go(b *testing.B) {
	src := []byte(goSource)
	p := sitter.NewParser()
	_ = p.SetLanguage(NativeLanguage("go"))
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tree, err := tsParse(p, src)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Native_ParseReuse_Python measures parse-only with parser reuse.
func BenchmarkTS_Native_ParseReuse_Python(b *testing.B) {
	src := []byte(pythonSource)
	p := sitter.NewParser()
	_ = p.SetLanguage(NativeLanguage("python"))
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tree, err := tsParse(p, src)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Native_ParseReuse_JavaScript measures parse-only with parser reuse.
func BenchmarkTS_Native_ParseReuse_JavaScript(b *testing.B) {
	src := []byte(jsSource)
	p := sitter.NewParser()
	_ = p.SetLanguage(NativeLanguage("javascript"))
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tree, err := tsParse(p, src)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Native_QueryExec_Go measures tree-sitter query compilation +
// execution against a pre-parsed Go AST.
func BenchmarkTS_Native_QueryExec_Go(b *testing.B) {
	src := []byte(goSource)
	lang := NativeLanguage("go")
	p := sitter.NewParser()
	_ = p.SetLanguage(lang)
	tree, err := tsParse(p, src)
	if err != nil {
		b.Fatal(err)
	}
	defer tree.Close()
	root := tree.RootNode()

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		q, qErr := sitter.NewQuery(lang, goQueryPattern)
		if qErr != nil {
			b.Fatal(qErr)
		}
		qc := sitter.NewQueryCursor()
		matches := qc.Matches(q, root, src)
		count := 0
		for {
			m := matches.Next()
			if m == nil {
				break
			}
			count += len(m.Captures)
		}
		qc.Close()
		if count == 0 {
			b.Fatal("no matches found")
		}
	}
	b.StopTimer()
}

// BenchmarkTS_Native_QueryExecReuse_Go measures query execution only,
// with a pre-compiled query (no compilation overhead per iteration).
func BenchmarkTS_Native_QueryExecReuse_Go(b *testing.B) {
	src := []byte(goSource)
	lang := NativeLanguage("go")
	p := sitter.NewParser()
	_ = p.SetLanguage(lang)
	tree, err := tsParse(p, src)
	if err != nil {
		b.Fatal(err)
	}
	defer tree.Close()
	root := tree.RootNode()

	q, qErr := sitter.NewQuery(lang, goQueryPattern)
	if qErr != nil {
		b.Fatal(qErr)
	}

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		qc := sitter.NewQueryCursor()
		matches := qc.Matches(q, root, src)
		count := 0
		for {
			m := matches.Next()
			if m == nil {
				break
			}
			count += len(m.Captures)
		}
		qc.Close()
		if count == 0 {
			b.Fatal("no matches found")
		}
	}
	b.StopTimer()
}

// BenchmarkTS_LangLookup_Native measures the cost of calling a native
// GetLanguage() function (current static approach).
func BenchmarkTS_LangLookup_Native(b *testing.B) {
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		lang := NativeLanguage("go")
		if lang == nil {
			b.Fatal("language not found")
		}
	}
	b.StopTimer()
}

// BenchmarkTS_LangLookup_Dynamic measures the cost of loading a grammar
// via the DynGrammarLoader (dynamic shared library approach).
// After the first load, this is a sync.Map cache hit.
func BenchmarkTS_LangLookup_Dynamic(b *testing.B) {
	loader := NewDynGrammarLoader(
		WithProjectDir("."),
	)
	defer loader.Close()

	// Warm up: first call loads the shared lib.
	if _, err := loader.Load("go"); err != nil {
		b.Skipf("dynamic Go grammar not available: %v", err)
	}

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		lang, err := loader.Load("go")
		if err != nil {
			b.Fatalf("load failed: %v", err)
		}
		if lang == nil {
			b.Fatal("nil language")
		}
	}
	b.StopTimer()
}

// BenchmarkTS_Pool_FullParse_Go measures parse performance using sync.Pool
// for the parser — this is the production path after the pool optimization.
func BenchmarkTS_Pool_FullParse_Go(b *testing.B) {
	src := []byte(goSource)
	lang := NativeLanguage("go")
	pool := sync.Pool{New: func() any { return sitter.NewParser() }}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p := pool.Get().(*sitter.Parser)
		_ = p.SetLanguage(lang)
		tree, err := tsParse(p, src)
		pool.Put(p)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Pool_FullParse_Python measures pool parse for Python.
func BenchmarkTS_Pool_FullParse_Python(b *testing.B) {
	src := []byte(pythonSource)
	lang := NativeLanguage("python")
	pool := sync.Pool{New: func() any { return sitter.NewParser() }}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p := pool.Get().(*sitter.Parser)
		_ = p.SetLanguage(lang)
		tree, err := tsParse(p, src)
		pool.Put(p)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Pool_FullParse_JavaScript measures pool parse for JS.
func BenchmarkTS_Pool_FullParse_JavaScript(b *testing.B) {
	src := []byte(jsSource)
	lang := NativeLanguage("javascript")
	pool := sync.Pool{New: func() any { return sitter.NewParser() }}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		p := pool.Get().(*sitter.Parser)
		_ = p.SetLanguage(lang)
		tree, err := tsParse(p, src)
		pool.Put(p)
		if err != nil {
			b.Fatal(err)
		}
		tree.Close()
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkTS_Pool_QueryExec_Go measures query cursor pool performance.
func BenchmarkTS_Pool_QueryExec_Go(b *testing.B) {
	src := []byte(goSource)
	lang := NativeLanguage("go")
	p := sitter.NewParser()
	_ = p.SetLanguage(lang)
	tree, err := tsParse(p, src)
	if err != nil {
		b.Fatal(err)
	}
	defer tree.Close()
	root := tree.RootNode()

	q, qErr := sitter.NewQuery(lang, goQueryPattern)
	if qErr != nil {
		b.Fatal(qErr)
	}

	pool := sync.Pool{New: func() any { return sitter.NewQueryCursor() }}

	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		qc := pool.Get().(*sitter.QueryCursor)
		matches := qc.Matches(q, root, src)
		count := 0
		for {
			m := matches.Next()
			if m == nil {
				break
			}
			count += len(m.Captures)
		}
		pool.Put(qc)
		if count == 0 {
			b.Fatal("no matches found")
		}
	}
	b.StopTimer()
}

// ===========================================================================
// ANTLR Benchmarks
// ===========================================================================

// BenchmarkANTLR_Native_PLSQL parses a PL/SQL procedure using the plsql Driver.
// This includes lexing, SLL→LL parsing, and tree conversion.
func BenchmarkANTLR_Native_PLSQL(b *testing.B) {
	src := []byte(plsqlSource)
	drv := &plsql.Driver{}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tree, err := drv.Parse(src)
		if err != nil {
			b.Fatal(err)
		}
		if tree == nil {
			b.Fatal("nil tree")
		}
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkANTLR_Native_PostgreSQL parses a PostgreSQL script using the postgresql Driver.
func BenchmarkANTLR_Native_PostgreSQL(b *testing.B) {
	src := []byte(pgSource)
	drv := &postgresql.Driver{}
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		tree, err := drv.Parse(src)
		if err != nil {
			b.Fatal(err)
		}
		if tree == nil {
			b.Fatal("nil tree")
		}
	}
	b.StopTimer()
	b.ReportMetric(float64(len(src))*float64(b.N)/(1024*1024*b.Elapsed().Seconds()), "MB/s")
}

// BenchmarkANTLR_Native_PLSQL_Preprocess measures just the PL/SQL
// preprocessor overhead (without the actual parse).
func BenchmarkANTLR_Native_PLSQL_Preprocess(b *testing.B) {
	src := plsqlSource
	b.ReportAllocs()
	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		result := plsql.Preprocess(src)
		if len(result) == 0 {
			b.Fatal("empty preprocess result")
		}
	}
	b.StopTimer()
}
