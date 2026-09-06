package testsupport

import (
	"crypto/sha256"
	"encoding/hex"
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
)

// FakeS3 implements the S3 REST subset used by internal/s3store and native Lance tests.
//
// It exists so no test in this repository needs a bucket, credentials, or the network to
// exercise the Hub's object backend. It is deliberately minimal: the operations the store
// actually calls, and nothing else.
type FakeS3 struct {
	mu            sync.Mutex
	bucket        string
	objects       map[string][]byte
	lastAccessKey string
	requests      map[string]int
}

// StartFakeS3 serves an in-memory bucket and returns it with its base endpoint. The server
// is closed when the test ends.
func StartFakeS3(t *testing.T, bucket string) (*FakeS3, string) {
	t.Helper()

	fake := &FakeS3{bucket: bucket, objects: map[string][]byte{}, requests: map[string]int{}}
	srv := httptest.NewServer(fake)
	t.Cleanup(srv.Close)

	t.Setenv("AWS_ACCESS_KEY_ID", "test-key")
	t.Setenv("AWS_SECRET_ACCESS_KEY", "test-secret")
	t.Setenv("AWS_EC2_METADATA_DISABLED", "true")

	return fake, srv.URL
}

// Keys names every stored object, sorted.
func (f *FakeS3) Keys() []string {
	f.mu.Lock()
	defer f.mu.Unlock()
	out := make([]string, 0, len(f.objects))
	for k := range f.objects {
		out = append(out, k)
	}
	sort.Strings(out)
	return out
}

// Object returns one stored object's bytes.
func (f *FakeS3) Object(key string) ([]byte, bool) {
	f.mu.Lock()
	defer f.mu.Unlock()
	data, ok := f.objects[key]
	return data, ok
}

// Put stores an object directly, for arranging state a test needs to read back.
func (f *FakeS3) Put(key string, data []byte) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.objects[key] = data
}

func (f *FakeS3) LastAccessKey() string {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.lastAccessKey
}

func (f *FakeS3) RequestCount(method, key string) int {
	f.mu.Lock()
	defer f.mu.Unlock()
	return f.requests[method+" "+key]
}

func (f *FakeS3) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	trimmed := strings.TrimPrefix(r.URL.Path, "/")
	bucket, key, _ := strings.Cut(trimmed, "/")
	if bucket != f.bucket {
		w.WriteHeader(http.StatusNotFound)
		return
	}

	f.mu.Lock()
	defer f.mu.Unlock()
	f.requests[r.Method+" "+key]++
	if authorization := r.Header.Get("Authorization"); authorization != "" {
		if _, credential, ok := strings.Cut(authorization, "Credential="); ok {
			f.lastAccessKey = strings.SplitN(credential, "/", 2)[0]
		}
	}

	switch {
	case r.Method == http.MethodHead && key == "":
		w.WriteHeader(http.StatusOK)

	case r.Method == http.MethodGet && r.URL.Query().Get("list-type") == "2":
		f.list(w, r.URL.Query().Get("prefix"), r.URL.Query().Get("continuation-token"), r.URL.Query().Get("max-keys"))

	case r.Method == http.MethodPost && r.URL.Query().Has("delete"):
		f.deleteObjects(w, r)

	case r.Method == http.MethodPut:
		if r.Header.Get("If-None-Match") == "*" {
			if _, exists := f.objects[key]; exists {
				writeS3Error(w, http.StatusPreconditionFailed, "PreconditionFailed")
				return
			}
		}
		if match := r.Header.Get("If-Match"); match != "" && match != etagFor(f.objects[key]) {
			writeS3Error(w, http.StatusPreconditionFailed, "PreconditionFailed")
			return
		}
		body, _ := io.ReadAll(r.Body)
		f.objects[key] = body
		w.Header().Set("ETag", etagFor(body))
		w.WriteHeader(http.StatusOK)

	case r.Method == http.MethodHead:
		data, ok := f.objects[key]
		if !ok {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Length", fmt.Sprint(len(data)))
		w.Header().Set("ETag", etagFor(data))
		w.WriteHeader(http.StatusOK)

	case r.Method == http.MethodGet:
		data, ok := f.objects[key]
		if !ok {
			w.WriteHeader(http.StatusNotFound)
			_, _ = w.Write([]byte(`<Error><Code>NoSuchKey</Code></Error>`))
			return
		}
		w.Header().Set("ETag", etagFor(data))
		if rawRange := strings.TrimPrefix(r.Header.Get("Range"), "bytes="); rawRange != "" {
			startText, endText, _ := strings.Cut(rawRange, "-")
			start, startErr := strconv.Atoi(startText)
			end := len(data) - 1
			var endErr error
			if endText != "" {
				end, endErr = strconv.Atoi(endText)
			}
			if startErr != nil || endErr != nil || start < 0 || end < start || start >= len(data) {
				w.WriteHeader(http.StatusRequestedRangeNotSatisfiable)
				return
			}
			if end >= len(data) {
				end = len(data) - 1
			}
			w.Header().Set("Content-Range", fmt.Sprintf("bytes %d-%d/%d", start, end, len(data)))
			w.Header().Set("Content-Length", strconv.Itoa(end-start+1))
			w.WriteHeader(http.StatusPartialContent)
			_, _ = w.Write(data[start : end+1])
			return
		}
		_, _ = w.Write(data)

	case r.Method == http.MethodDelete:
		if match := r.Header.Get("If-Match"); match != "" && match != etagFor(f.objects[key]) {
			writeS3Error(w, http.StatusPreconditionFailed, "PreconditionFailed")
			return
		}
		delete(f.objects, key)
		w.WriteHeader(http.StatusNoContent)

	default:
		w.WriteHeader(http.StatusBadRequest)
	}
}

func (f *FakeS3) list(w http.ResponseWriter, prefix, cursor, maxKeysText string) {
	type contents struct {
		Key          string `xml:"Key"`
		LastModified string `xml:"LastModified"`
		Size         int64  `xml:"Size"`
		ETag         string `xml:"ETag"`
		StorageClass string `xml:"StorageClass"`
	}
	type result struct {
		XMLName               xml.Name   `xml:"ListBucketResult"`
		IsTruncated           bool       `xml:"IsTruncated"`
		NextContinuationToken string     `xml:"NextContinuationToken,omitempty"`
		Contents              []contents `xml:"Contents"`
	}

	keys := make([]string, 0, len(f.objects))
	for k := range f.objects {
		if strings.HasPrefix(k, prefix) {
			keys = append(keys, k)
		}
	}
	sort.Strings(keys)
	start := 0
	if cursor != "" {
		for start < len(keys) && keys[start] <= cursor {
			start++
		}
	}
	limit := len(keys)
	if parsed, err := strconv.Atoi(maxKeysText); err == nil && parsed > 0 && parsed < limit {
		limit = parsed
	}
	end := min(start+limit, len(keys))

	res := result{IsTruncated: end < len(keys)}
	if res.IsTruncated && end > start {
		res.NextContinuationToken = keys[end-1]
	}
	for _, k := range keys[start:end] {
		sum := sha256.Sum256(f.objects[k])
		res.Contents = append(res.Contents, contents{
			Key:          k,
			LastModified: "2026-01-01T00:00:00.000Z",
			Size:         int64(len(f.objects[k])),
			ETag:         `"` + hex.EncodeToString(sum[:]) + `"`,
			StorageClass: "STANDARD",
		})
	}
	w.Header().Set("Content-Type", "application/xml")
	_ = xml.NewEncoder(w).Encode(res)
}

func etagFor(data []byte) string {
	if data == nil {
		return ""
	}
	sum := sha256.Sum256(data)
	return `"` + hex.EncodeToString(sum[:]) + `"`
}

func writeS3Error(w http.ResponseWriter, status int, code string) {
	w.Header().Set("Content-Type", "application/xml")
	w.WriteHeader(status)
	_, _ = fmt.Fprintf(w, "<Error><Code>%s</Code></Error>", code)
}

func (f *FakeS3) deleteObjects(w http.ResponseWriter, r *http.Request) {
	var payload struct {
		Objects []struct {
			Key string `xml:"Key"`
		} `xml:"Object"`
	}
	body, _ := io.ReadAll(r.Body)
	if err := xml.Unmarshal(body, &payload); err != nil {
		w.WriteHeader(http.StatusBadRequest)
		return
	}
	for _, o := range payload.Objects {
		delete(f.objects, o.Key)
	}
	w.Header().Set("Content-Type", "application/xml")
	_, _ = w.Write([]byte(`<DeleteResult></DeleteResult>`))
}
