package ai

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"strings"
	"time"
)

// httpClient is shared by every remote embedding and rerank client (OpenAI-shape, Cohere,
// Voyage, Google, Jina all do JSON-over-HTTPS POST with a Bearer or API-key header, so one
// client and one request helper serve all of them).
//
// The timeout here is a safety net against a provider that never answers at all — it is NOT the
// mechanism a caller relies on for timing a call out. Every request is built with
// http.NewRequestWithContext, so the caller's own context still governs cancellation and can cut
// a request short well before this fires.
var httpClient = &http.Client{Timeout: 120 * time.Second}

func postJSON(ctx context.Context, client *http.Client, url string, setAuth func(*http.Request), reqBody, respBody any) error {
	buf, err := json.Marshal(reqBody)
	if err != nil {
		return fmt.Errorf("encode request body for %s: %w", url, err)
	}

	req, err := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(buf))
	if err != nil {
		return fmt.Errorf("build request to %s: %w", url, err)
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("Accept", "application/json")
	if setAuth != nil {
		setAuth(req)
	}

	resp, err := client.Do(req)
	if err != nil {
		return fmt.Errorf("request to %s: %w", url, err)
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return fmt.Errorf("read response from %s: %w", url, err)
	}

	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		return fmt.Errorf("%s returned %s: %s", url, resp.Status, truncateForError(strings.TrimSpace(string(data))))
	}

	if respBody == nil {
		return nil
	}
	if err := json.Unmarshal(data, respBody); err != nil {
		return fmt.Errorf("decode response from %s: %w (body: %s)", url, err, truncateForError(string(data)))
	}
	return nil
}

func truncateForError(s string) string {
	const max = 500
	if len(s) <= max {
		return s
	}
	return s[:max] + "…"
}

func bearerAuth(apiKey string) func(*http.Request) {
	if strings.TrimSpace(apiKey) == "" {
		return nil
	}
	return func(req *http.Request) {
		req.Header.Set("Authorization", "Bearer "+apiKey)
	}
}

func headerAuth(name, value string) func(*http.Request) {
	if strings.TrimSpace(value) == "" {
		return nil
	}
	return func(req *http.Request) {
		req.Header.Set(name, value)
	}
}
