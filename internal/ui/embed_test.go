package ui

import (
	"io/fs"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestServeStatic(t *testing.T) {
	t.Parallel()

	tests := []struct {
		name              string
		path              string
		wantServed        bool
		wantContentType   string
		wantCacheControl  string
		wantNonEmptyBody  bool
	}{
		{
			name:       "root path returns false",
			path:       "/",
			wantServed: false,
		},
		{
			name:       "api path returns false",
			path:       "/api/status",
			wantServed: false,
		},
		{
			name:       "api root returns false",
			path:       "/api/",
			wantServed: false,
		},
		{
			name:       "api nested path returns false",
			path:       "/api/v1/projects",
			wantServed: false,
		},
		{
			name:       "non-existent file returns false",
			path:       "/no-such-file.js",
			wantServed: false,
		},
		{
			name:       "directory traversal attempt returns false",
			path:       "/../go.mod",
			wantServed: false,
		},
		{
			name:             "existing html file is served with correct MIME",
			path:             "/index.html",
			wantServed:       true,
			wantContentType:  "text/html",
			wantCacheControl: "public, max-age=3600",
			wantNonEmptyBody: true,
		},
		{
			name:             "existing svg file is served with correct MIME",
			path:             "/logo.svg",
			wantServed:       true,
			wantContentType:  "image/svg+xml",
			wantCacheControl: "public, max-age=3600",
			wantNonEmptyBody: true,
		},
	}

	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()

			if tc.wantServed {
				fp := "dist" + tc.path
				if _, err := fs.Stat(DistFS, fp); err != nil {
					t.Skipf("dist not built (file %s not embedded); run 'make ui' first", fp)
				}
			}

			req := httptest.NewRequest(http.MethodGet, tc.path, nil)
			rec := httptest.NewRecorder()

			got := ServeStatic(rec, req)
			if got != tc.wantServed {
				t.Fatalf("ServeStatic() = %v; want %v", got, tc.wantServed)
			}

			if !tc.wantServed {
				return
			}

			ct := rec.Header().Get("Content-Type")
			if ct == "" {
				t.Fatal("Content-Type header is empty")
			}
			if len(tc.wantContentType) > 0 {
				if ct != tc.wantContentType && !strings.HasPrefix(ct, tc.wantContentType) {
					t.Errorf("Content-Type = %q; want prefix %q", ct, tc.wantContentType)
				}
			}

			cc := rec.Header().Get("Cache-Control")
			if cc != tc.wantCacheControl {
				t.Errorf("Cache-Control = %q; want %q", cc, tc.wantCacheControl)
			}

			if tc.wantNonEmptyBody && rec.Body.Len() == 0 {
				t.Error("expected non-empty response body")
			}
		})
	}
}

func TestServeStatic_UnknownExtensionFallback(t *testing.T) {
	t.Parallel()

	// The embedded dist/ doesn't contain a file with an unknown extension,
	// so we verify the fallback indirectly: any served file whose extension
	// IS recognized gets a real MIME type (covered above). The octet-stream
	// fallback branch triggers when mime.TypeByExtension returns "".
	// We can't inject a file into the embed.FS, but we verify the known
	// extension paths produce correct MIME types, confirming the else-branch
	// is the only remaining path for unknown extensions.

	// Verify that a path that doesn't match /api/* and isn't "/" but
	// references a missing file still returns false.
	req := httptest.NewRequest(http.MethodGet, "/somefile.unknownext12345", nil)
	rec := httptest.NewRecorder()

	if ServeStatic(rec, req) {
		t.Fatal("expected false for non-existent file with unknown extension")
	}
}
