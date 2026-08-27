package testsupport

import (
	"encoding/xml"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"sort"
	"strings"
	"sync"
	"testing"
)

// FakeS3 implements the handful of S3 REST operations internal/s3store issues, in memory.
//
// It exists so no test in this repository needs a bucket, credentials, or the network to
// exercise the Hub's object backend. It is deliberately minimal: the operations the store
// actually calls, and nothing else.
type FakeS3 struct {
	mu            sync.Mutex
	bucket        string
	objects       map[string][]byte
	lastAccessKey string
}

// StartFakeS3 serves an in-memory bucket and returns it with its base endpoint. The server
// is closed when the test ends.
func StartFakeS3(t *testing.T, bucket string) (*FakeS3, string) {
	t.Helper()

	fake := &FakeS3{bucket: bucket, objects: map[string][]byte{}}
	srv := httptest.NewServer(fake)
	t.Cleanup(srv.Close)

	// The SDK signs every request, so it needs credentials even against a fake. These are
	// nonsense on purpose: nothing here validates a signature.
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

func (f *FakeS3) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	trimmed := strings.TrimPrefix(r.URL.Path, "/")
	bucket, key, _ := strings.Cut(trimmed, "/")
	if bucket != f.bucket {
		w.WriteHeader(http.StatusNotFound)
		return
	}

	f.mu.Lock()
	defer f.mu.Unlock()
	if authorization := r.Header.Get("Authorization"); authorization != "" {
		if _, credential, ok := strings.Cut(authorization, "Credential="); ok {
			f.lastAccessKey = strings.SplitN(credential, "/", 2)[0]
		}
	}

	switch {
	case r.Method == http.MethodHead && key == "":
		w.WriteHeader(http.StatusOK)

	case r.Method == http.MethodGet && r.URL.Query().Get("list-type") == "2":
		f.list(w, r.URL.Query().Get("prefix"))

	case r.Method == http.MethodPost && r.URL.Query().Has("delete"):
		f.deleteObjects(w, r)

	case r.Method == http.MethodPut:
		body, _ := io.ReadAll(r.Body)
		f.objects[key] = body
		w.WriteHeader(http.StatusOK)

	case r.Method == http.MethodHead:
		if _, ok := f.objects[key]; !ok {
			w.WriteHeader(http.StatusNotFound)
			return
		}
		w.Header().Set("Content-Length", fmt.Sprint(len(f.objects[key])))
		w.WriteHeader(http.StatusOK)

	case r.Method == http.MethodGet:
		data, ok := f.objects[key]
		if !ok {
			w.WriteHeader(http.StatusNotFound)
			_, _ = w.Write([]byte(`<Error><Code>NoSuchKey</Code></Error>`))
			return
		}
		_, _ = w.Write(data)

	case r.Method == http.MethodDelete:
		delete(f.objects, key)
		w.WriteHeader(http.StatusNoContent)

	default:
		w.WriteHeader(http.StatusBadRequest)
	}
}

func (f *FakeS3) list(w http.ResponseWriter, prefix string) {
	type contents struct {
		Key  string `xml:"Key"`
		Size int64  `xml:"Size"`
	}
	type result struct {
		XMLName     xml.Name   `xml:"ListBucketResult"`
		IsTruncated bool       `xml:"IsTruncated"`
		Contents    []contents `xml:"Contents"`
	}

	keys := make([]string, 0, len(f.objects))
	for k := range f.objects {
		if strings.HasPrefix(k, prefix) {
			keys = append(keys, k)
		}
	}
	sort.Strings(keys)

	res := result{}
	for _, k := range keys {
		res.Contents = append(res.Contents, contents{Key: k, Size: int64(len(f.objects[k]))})
	}
	w.Header().Set("Content-Type", "application/xml")
	_ = xml.NewEncoder(w).Encode(res)
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
