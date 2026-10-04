package sessionhook

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"net/http"
	"strconv"
	"time"
)

// DaemonPath is the daemon endpoint that applies the store-backed half of a hook event.
// The lightweight hook binary renders events itself and asks the daemon only for what
// needs a store — the Task lifecycle effect and mandatory memory — so a hook, which
// agents fire on every tool call, never starts a process that opens the stores.
const DaemonPath = "/internal/session-store"

// StoreRequest is the store-backed work one hook event needs.
type StoreRequest struct {
	Format     string `json:"format"`
	ProjectDir string `json:"project_dir"`
	Input      []byte `json:"input"`
	// Task applies the event's Task lifecycle effect (reconcile, heartbeat or release).
	Task bool `json:"task"`
	// Mandatory reads the project's and user's mandatory memory.
	Mandatory bool `json:"mandatory"`
}

// StoreResult is what the store-backed half returns for rendering.
type StoreResult struct {
	// Mandatory is the rendered mandatory memory; MandatoryLoaded is false when it could
	// not be read, so the payload tells the agent to fetch it itself.
	Mandatory       string `json:"mandatory,omitempty"`
	MandatoryLoaded bool   `json:"mandatory_loaded,omitempty"`
}

// StoreResponse is the daemon's answer: the result, or the error the core command
// would have reported.
type StoreResponse struct {
	StoreResult
	Error string `json:"error,omitempty"`
}

// ErrDaemonUnavailable reports that the daemon could not be asked at all; the caller can
// still handle the event in the core. An error the daemon returned is the event's own
// result and must not be retried elsewhere — the Task side effects already happened.
var ErrDaemonUnavailable = errors.New("daemon unavailable for session hook")

// ApplyRemote asks the daemon listening on port, authenticated with key, to apply req.
func ApplyRemote(ctx context.Context, port int, key string, req StoreRequest) (StoreResult, error) {
	body, err := json.Marshal(req)
	if err != nil {
		return StoreResult{}, err
	}
	ctx, cancel := context.WithTimeout(ctx, 60*time.Second)
	defer cancel()
	url := "http://127.0.0.1:" + strconv.Itoa(port) + DaemonPath
	httpReq, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(body))
	if err != nil {
		return StoreResult{}, fmt.Errorf("%w: %w", ErrDaemonUnavailable, err)
	}
	httpReq.Header.Set("Authorization", "Bearer "+key)
	httpReq.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(httpReq)
	if err != nil {
		return StoreResult{}, fmt.Errorf("%w: %w", ErrDaemonUnavailable, err)
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode == http.StatusNotFound || resp.StatusCode == http.StatusUnauthorized {
		// An older daemon without the endpoint, or a rotated key: hand the event to the core.
		return StoreResult{}, fmt.Errorf("%w: HTTP %d", ErrDaemonUnavailable, resp.StatusCode)
	}
	var out StoreResponse
	if err := json.NewDecoder(resp.Body).Decode(&out); err != nil {
		return StoreResult{}, fmt.Errorf("%w: decode: %w", ErrDaemonUnavailable, err)
	}
	if out.Error != "" {
		return StoreResult{}, errors.New(out.Error)
	}
	return out.StoreResult, nil
}
