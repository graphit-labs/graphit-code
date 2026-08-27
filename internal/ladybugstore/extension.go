package ladybugstore

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
)

// ExtHTTPFS reads and writes files over HTTP(S) and S3, which is what lets a graph table
// declare `storage = 's3://…'` and be queried without ingesting anything.
const ExtHTTPFS = "httpfs"

// ExtensionDir is where the launcher extracts the LadybugDB extensions it carries.
//
// The extensions travel inside the binary and are loaded from this directory by path. The
// engine's own INSTALL would fetch them from the extension server at query time, which is a
// network call in the middle of a user's query and a version that may not be published —
// see docs/tasks/hub-em-s3-icebug-e-lancedb.md.
func ExtensionDir() string {
	d := brand.RuntimeDir(version.Version)
	if d == "" {
		return ""
	}
	return filepath.Join(d, "lbug")
}

// ExtensionPath is the file the launcher extracted for one extension.
func ExtensionPath(name string) string {
	dir := ExtensionDir()
	if dir == "" {
		return ""
	}
	return filepath.Join(dir, name+".lbug_extension")
}

// LoadExtensions loads each named extension from the launcher payload and verifies that it
// actually loaded.
//
// SAFETY: LOAD EXTENSION reports success when it loaded nothing. Measured on liblbug 0.18.2
// (internal/ladybugstore/httpfs_probe_test.go): both `INSTALL httpfs` and
// `LOAD EXTENSION httpfs` returned no error while show_loaded_extensions() stayed empty and
// the extension's functions did not exist. The verification below is the ONLY thing that
// separates a loaded extension from a silent no-op, and without it the failure surfaces
// much later as a missing function or an unreadable remote table.
// ExtensionLoadStatement validates an extension binary and returns the statement that loads it.
//
// Exposed so a caller with its own connection — internal/ast has one, and it is not a Store —
// runs the SAME validation instead of writing `LOAD EXTENSION` itself. That validation is not
// optional: pointing LOAD EXTENSION at a small HTML file, which is what a 404 produces, kills the
// process with SIGBUS inside cgo and no Go recover can catch it.
func ExtensionLoadStatement(name string) (string, error) {
	path := ExtensionPath(name)
	if path == "" {
		return "", fmt.Errorf("load %s extension: no runtime directory (is HOME set?)", name)
	}
	if _, err := os.Stat(path); err != nil {
		return "", fmt.Errorf("load %s extension: %s is missing — the launcher payload did not carry it: %w", name, path, err)
	}
	if err := validateExtensionFile(path); err != nil {
		return "", fmt.Errorf("load %s extension: %w", name, err)
	}
	return "LOAD EXTENSION '" + EscapeLiteral(path) + "'", nil
}

// S3ConfigStatements are what httpfs reads when it resolves an s3:// URI.
//
// Returned rather than executed for the same reason as ExtensionLoadStatement: two callers, one
// definition of what has to be set.
func S3ConfigStatements(creds S3Credentials) []string {
	var out []string
	add := func(k, v string) {
		if v != "" {
			out = append(out, fmt.Sprintf("CALL %s='%s'", k, EscapeLiteral(v)))
		}
	}
	add("s3_access_key_id", creds.AccessKeyID)
	add("s3_secret_access_key", creds.SecretAccessKey)
	add("s3_session_token", creds.SessionToken)
	add("s3_region", creds.Region)
	add("s3_endpoint", creds.Endpoint)
	if creds.PathStyle {
		out = append(out, "CALL s3_url_style='path'")
	}
	if creds.DisableSSL {
		out = append(out, "CALL s3_disable_ssl=true")
	}
	return out
}

func (s *Store) LoadExtensions(names ...string) error {
	for _, name := range names {
		path := ExtensionPath(name)
		if path == "" {
			return fmt.Errorf("load %s extension: no runtime directory (is HOME set?)", name)
		}
		if _, err := os.Stat(path); err != nil {
			return fmt.Errorf("load %s extension: %s is missing — the launcher payload did not carry it: %w", name, path, err)
		}
		if err := validateExtensionFile(path); err != nil {
			return fmt.Errorf("load %s extension: %w", name, err)
		}
		if err := s.Exec("LOAD EXTENSION '"+EscapeLiteral(path)+"'", nil); err != nil {
			return fmt.Errorf("load %s extension from %s: %w", name, path, err)
		}

		loaded, err := s.LoadedExtensions()
		if err != nil {
			return fmt.Errorf("verifying %s extension: %w", name, err)
		}
		if !containsFold(loaded, name) {
			return fmt.Errorf("load %s extension: the engine reported success but %s is not loaded (loaded: %v) — the extension binary is probably built for a different engine version",
				name, name, loaded)
		}
	}
	return nil
}

// validateExtensionFile refuses anything that is not a native object file.
//
// SAFETY: this check cannot be skipped and cannot be replaced by handling the error from
// LOAD EXTENSION. Measured: pointing LOAD EXTENSION at a small HTML file — exactly what a
// 404 from the extension server produces — does not return an error, it kills the process
// with SIGBUS inside cgo, which no Go recover can catch. The build fetches with `curl -f`
// for the same reason; this is the second line of the same defence.
func validateExtensionFile(path string) error {
	info, err := os.Stat(path)
	if err != nil {
		return err
	}
	if info.Size() < minExtensionBytes {
		return fmt.Errorf("%s is %d bytes, too small to be an extension — a failed download probably left an error page there", path, info.Size())
	}

	f, err := os.Open(path)
	if err != nil {
		return err
	}
	defer f.Close()

	var magic [4]byte
	if _, err := io.ReadFull(f, magic[:]); err != nil {
		return fmt.Errorf("reading %s: %w", path, err)
	}
	if !isObjectFileMagic(magic) {
		return fmt.Errorf("%s does not start like a native library (% x) — a failed download probably left an error page there", path, magic)
	}
	return nil
}

// minExtensionBytes is far below the smallest real extension (the published httpfs binaries
// range from ~780 KB to ~14 MB) and far above any error page.
const minExtensionBytes = 64 << 10

func isObjectFileMagic(m [4]byte) bool {
	switch {
	case m[0] == 0x7f && m[1] == 'E' && m[2] == 'L' && m[3] == 'F': // ELF
		return true
	case m[0] == 'M' && m[1] == 'Z': // PE
		return true
	case m == [4]byte{0xfe, 0xed, 0xfa, 0xce}, m == [4]byte{0xfe, 0xed, 0xfa, 0xcf}, // Mach-O
		m == [4]byte{0xce, 0xfa, 0xed, 0xfe}, m == [4]byte{0xcf, 0xfa, 0xed, 0xfe},
		m == [4]byte{0xca, 0xfe, 0xba, 0xbe}, m == [4]byte{0xbe, 0xba, 0xfe, 0xca}: // Mach-O fat
		return true
	}
	return false
}

// LoadedExtensions names the extensions this connection actually has.
func (s *Store) LoadedExtensions() ([]string, error) {
	rows, err := s.Query("CALL show_loaded_extensions() RETURN *", nil)
	if err != nil {
		return nil, err
	}
	out := make([]string, 0, len(rows))
	for _, r := range rows {
		if name := Str(r["extension name"]); name != "" {
			out = append(out, name)
		}
	}
	return out, nil
}

// S3Credentials is what httpfs needs to reach a bucket.
//
// SAFETY: Secret is a live credential. It reaches the engine inside a statement, so no
// caller may log the statements ConfigureS3 issues, and nothing here returns them.
type S3Credentials struct {
	AccessKeyID     string
	SecretAccessKey string
	SessionToken    string
	Region          string
	// Endpoint is host[:port], WITHOUT a scheme. The engine prepends one; DisableSSL chooses it.
	//
	// MEASURED, because guessing cost three wrong attempts: passing the scheme in the endpoint is
	// ACCEPTED and then produces `https://http://localhost:9000/…`, so the engine prefixes
	// unconditionally. And the option is not called what you would expect — `s3_use_ssl`,
	// `s3_ssl`, `http_use_ssl`, `s3_scheme`, `s3_protocol`, `s3_insecure`, `s3_verify_ssl` and
	// `s3_use_tls` all return `Binder exception: Invalid option name`. The one that exists is
	// `s3_disable_ssl`.
	Endpoint string
	// DisableSSL reaches a plain-HTTP endpoint, such as a local MinIO.
	DisableSSL bool
	// PathStyle addresses the bucket in the path rather than the host, which MinIO and most
	// S3-compatible servers require.
	PathStyle bool
}

// ConfigureS3 applies the credentials httpfs reads when it resolves an s3:// URI.
//
// The documented `CALL s3_credential(key_id=..., secret=..., region=...)` does NOT bind in
// this engine — measured: "Catalog exception: function s3_credential does not exist". S3
// settings are extension OPTIONS, one statement each, in the same shape as the documented
// `CALL http_cache_file=true`.
func (s *Store) ConfigureS3(creds S3Credentials) error {
	options := []struct {
		key, value string
	}{
		{"s3_access_key_id", creds.AccessKeyID},
		{"s3_secret_access_key", creds.SecretAccessKey},
		{"s3_session_token", creds.SessionToken},
		{"s3_region", creds.Region},
		{"s3_endpoint", creds.Endpoint},
	}
	for _, o := range options {
		if o.value == "" {
			continue
		}
		if err := s.Exec(fmt.Sprintf("CALL %s='%s'", o.key, EscapeLiteral(o.value)), nil); err != nil {
			// The key is safe to name; the value is not.
			return fmt.Errorf("setting %s: %w", o.key, err)
		}
	}
	if creds.PathStyle {
		if err := s.Exec("CALL s3_url_style='path'", nil); err != nil {
			return fmt.Errorf("setting s3_url_style: %w", err)
		}
	}
	return nil
}

// EnableRemoteCache turns on httpfs's local read cache, which keeps a re-read of the same
// remote range off the network.
func (s *Store) EnableRemoteCache() error {
	return s.Exec("CALL http_cache_file=true", nil)
}

func containsFold(haystack []string, needle string) bool {
	for _, h := range haystack {
		if strings.EqualFold(h, needle) {
			return true
		}
	}
	return false
}
