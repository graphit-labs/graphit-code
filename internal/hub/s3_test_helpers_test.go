package hub

import (
	"encoding/xml"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"sort"
	"strconv"
	"strings"
	"sync"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

type testObjectServer struct {
	mu      sync.Mutex
	objects map[string][]byte
	server  *httptest.Server
}

func (s *testObjectServer) Keys() []string {
	s.mu.Lock()
	defer s.mu.Unlock()
	keys := make([]string, 0, len(s.objects))
	for key := range s.objects {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	return keys
}

func newTestS3Store(t *testing.T) (*S3Store, *testObjectServer) {
	t.Helper()
	fake := &testObjectServer{objects: map[string][]byte{}}
	fake.server = httptest.NewServer(http.HandlerFunc(fake.serveHTTP))
	t.Cleanup(fake.server.Close)
	cfg := config.S3Config{Bucket: "test", Region: "us-east-1", Endpoint: fake.server.URL, AccessKeyID: "test", SecretAccessKey: "test"}
	objects, err := s3store.New(t.Context(), cfg)
	if err != nil {
		t.Fatal(err)
	}
	return &S3Store{objects: objects, cfg: cfg, cacheBase: t.TempDir()}, fake
}

func (s *testObjectServer) serveHTTP(w http.ResponseWriter, r *http.Request) {
	s.mu.Lock()
	defer s.mu.Unlock()
	path := strings.TrimPrefix(r.URL.Path, "/")
	parts := strings.SplitN(path, "/", 2)
	if len(parts) == 0 || parts[0] != "test" {
		http.NotFound(w, r)
		return
	}
	key := ""
	if len(parts) == 2 {
		key = parts[1]
	}
	if r.Method == http.MethodHead && key == "" {
		w.WriteHeader(http.StatusOK)
		return
	}
	if r.Method == http.MethodGet && r.URL.Query().Get("list-type") == "2" {
		s.serveList(w, r)
		return
	}
	if r.Method == http.MethodPost && r.URL.Query().Has("delete") {
		s.serveDeleteObjects(w, r)
		return
	}
	switch r.Method {
	case http.MethodGet:
		data, ok := s.objects[key]
		if !ok {
			writeS3Error(w, http.StatusNotFound, "NoSuchKey")
			return
		}
		w.Header().Set("ETag", fmt.Sprintf("\"%d\"", len(data)))
		_, _ = w.Write(data)
	case http.MethodHead:
		if _, ok := s.objects[key]; !ok {
			w.WriteHeader(http.StatusNotFound)
		}
	case http.MethodPut:
		if match := r.Header.Get("If-Match"); match != "" {
			current, exists := s.objects[key]
			if !exists || match != fmt.Sprintf("\"%d\"", len(current)) {
				writeS3Error(w, http.StatusPreconditionFailed, "PreconditionFailed")
				return
			}
		}
		if r.Header.Get("If-None-Match") == "*" {
			if _, exists := s.objects[key]; exists {
				writeS3Error(w, http.StatusPreconditionFailed, "PreconditionFailed")
				return
			}
		}
		data, _ := io.ReadAll(r.Body)
		s.objects[key] = data
		w.Header().Set("ETag", fmt.Sprintf("\"%d\"", len(data)))
	case http.MethodDelete:
		delete(s.objects, key)
		w.WriteHeader(http.StatusNoContent)
	default:
		http.Error(w, "unsupported operation", http.StatusBadRequest)
	}
}

func (s *testObjectServer) serveList(w http.ResponseWriter, r *http.Request) {
	prefix := r.URL.Query().Get("prefix")
	keys := make([]string, 0)
	for key := range s.objects {
		if strings.HasPrefix(key, prefix) {
			keys = append(keys, key)
		}
	}
	sort.Strings(keys)
	start, _ := strconv.Atoi(r.URL.Query().Get("continuation-token"))
	if start > len(keys) {
		start = len(keys)
	}
	keys = keys[start:]
	limit, _ := strconv.Atoi(r.URL.Query().Get("max-keys"))
	truncated := limit > 0 && len(keys) > limit
	if truncated {
		keys = keys[:limit]
	}
	type content struct {
		Key  string `xml:"Key"`
		Size int    `xml:"Size"`
		ETag string `xml:"ETag"`
	}
	result := struct {
		XMLName               xml.Name  `xml:"ListBucketResult"`
		Name                  string    `xml:"Name"`
		Prefix                string    `xml:"Prefix"`
		KeyCount              int       `xml:"KeyCount"`
		MaxKeys               int       `xml:"MaxKeys"`
		IsTruncated           bool      `xml:"IsTruncated"`
		NextContinuationToken string    `xml:"NextContinuationToken,omitempty"`
		Contents              []content `xml:"Contents"`
	}{Name: "test", Prefix: prefix, KeyCount: len(keys), MaxKeys: limit, IsTruncated: truncated}
	if truncated {
		result.NextContinuationToken = strconv.Itoa(start + limit)
	}
	for _, key := range keys {
		data := s.objects[key]
		result.Contents = append(result.Contents, content{Key: key, Size: len(data), ETag: fmt.Sprintf("\"%d\"", len(data))})
	}
	w.Header().Set("Content-Type", "application/xml")
	_ = xml.NewEncoder(w).Encode(result)
}

func (s *testObjectServer) serveDeleteObjects(w http.ResponseWriter, r *http.Request) {
	var input struct {
		Objects []struct {
			Key string `xml:"Key"`
		} `xml:"Object"`
	}
	_ = xml.NewDecoder(r.Body).Decode(&input)
	for _, object := range input.Objects {
		delete(s.objects, object.Key)
	}
	w.Header().Set("Content-Type", "application/xml")
	_, _ = io.WriteString(w, "<DeleteResult></DeleteResult>")
}

func writeS3Error(w http.ResponseWriter, status int, code string) {
	w.Header().Set("Content-Type", "application/xml")
	w.WriteHeader(status)
	_, _ = fmt.Fprintf(w, "<Error><Code>%s</Code></Error>", code)
}
