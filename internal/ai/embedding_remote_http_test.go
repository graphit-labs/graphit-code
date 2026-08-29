package ai

import (
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestPostJSON_SendsBodyAndAuthHeader(t *testing.T) {
	t.Parallel()
	var gotAuth, gotContentType string
	var gotBody map[string]any
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotAuth = r.Header.Get("Authorization")
		gotContentType = r.Header.Get("Content-Type")
		_ = json.NewDecoder(r.Body).Decode(&gotBody)
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"ok":true}`))
	}))
	defer srv.Close()

	var resp struct {
		OK bool `json:"ok"`
	}
	err := postJSON(context.Background(), httpClient, srv.URL, bearerAuth("secret-key"),
		map[string]string{"hello": "world"}, &resp)
	if err != nil {
		t.Fatalf("postJSON: %v", err)
	}
	if !resp.OK {
		t.Error("response body was not decoded into respBody")
	}
	if gotContentType != "application/json" {
		t.Errorf("Content-Type = %q, want application/json", gotContentType)
	}
	if gotAuth != "Bearer secret-key" {
		t.Errorf("Authorization = %q, want %q", gotAuth, "Bearer secret-key")
	}
	if gotBody["hello"] != "world" {
		t.Errorf("request body = %v, want hello=world", gotBody)
	}
}

func TestPostJSON_NoAuthHeaderWhenSetAuthIsNil(t *testing.T) {
	t.Parallel()
	var sawAuthHeader bool
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Header.Get("Authorization") != "" {
			sawAuthHeader = true
		}
		_, _ = w.Write([]byte(`{}`))
	}))
	defer srv.Close()

	if err := postJSON(context.Background(), httpClient, srv.URL, nil, map[string]string{}, nil); err != nil {
		t.Fatalf("postJSON: %v", err)
	}
	if sawAuthHeader {
		t.Error("an Authorization header was sent despite setAuth being nil")
	}
}

func TestBearerAuth_EmptyKeyMeansNoAuthFunc(t *testing.T) {
	t.Parallel()
	if bearerAuth("") != nil {
		t.Error("bearerAuth(\"\") should be nil, not a func that sends an empty Bearer token")
	}
	if bearerAuth("k") == nil {
		t.Error("bearerAuth with a non-empty key should return a setAuth func")
	}
}

func TestHeaderAuth_SetsNamedHeader(t *testing.T) {
	t.Parallel()
	var got string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		got = r.Header.Get("x-goog-api-key")
		_, _ = w.Write([]byte(`{}`))
	}))
	defer srv.Close()

	err := postJSON(context.Background(), httpClient, srv.URL, headerAuth("x-goog-api-key", "abc"),
		map[string]string{}, nil)
	if err != nil {
		t.Fatalf("postJSON: %v", err)
	}
	if got != "abc" {
		t.Errorf("x-goog-api-key = %q, want %q", got, "abc")
	}
}

// A bad key or a malformed request must be diagnosable from the error alone: the provider's own
// response body is the only thing that says WHICH of those it was.
func TestPostJSON_NonOKStatusIncludesProviderBodyInError(t *testing.T) {
	t.Parallel()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
		_, _ = w.Write([]byte(`{"error":{"message":"invalid api key"}}`))
	}))
	defer srv.Close()

	err := postJSON(context.Background(), httpClient, srv.URL, nil, map[string]string{}, nil)
	if err == nil {
		t.Fatal("expected an error for a 401 response")
	}
	if !strings.Contains(err.Error(), "invalid api key") {
		t.Errorf("error = %v, want it to contain the provider's response body", err)
	}
}

func TestPostJSON_MalformedJSONResponseIsAnError(t *testing.T) {
	t.Parallel()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		_, _ = w.Write([]byte(`this is not json`))
	}))
	defer srv.Close()

	var resp struct{}
	err := postJSON(context.Background(), httpClient, srv.URL, nil, map[string]string{}, &resp)
	if err == nil {
		t.Fatal("expected a decode error for malformed JSON")
	}
}

// The caller's own context must govern cancellation — the shared client timeout is only a safety
// net, not the mechanism relied on here.
func TestPostJSON_RespectsCallerContextCancellation(t *testing.T) {
	t.Parallel()
	block := make(chan struct{})
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		<-block
	}))
	defer func() {
		close(block)
		srv.Close()
	}()

	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	err := postJSON(ctx, httpClient, srv.URL, nil, map[string]string{}, nil)
	if err == nil {
		t.Fatal("expected an error for an already-cancelled context")
	}
}
