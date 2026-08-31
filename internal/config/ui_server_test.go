package config

import (
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestUIConfigDefaultsAndProjectPrecedence(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(brand.EnvVar("UI_HOST"), "")
	t.Setenv(brand.EnvVar("UI_ALLOWED_ORIGINS"), "")

	if got := ResolveUIHost(nil, nil); got != "127.0.0.1" {
		t.Fatalf("default host = %q; want 127.0.0.1", got)
	}
	if err := SetGlobalConfigValue("ui.host", "192.0.2.1"); err != nil {
		t.Fatalf("set global UI host: %v", err)
	}
	if err := SetGlobalConfigValue("ui.allowed_origins", "https://global.test"); err != nil {
		t.Fatalf("set global UI origins: %v", err)
	}
	if got := ResolveUIHost(nil, nil); got != "192.0.2.1" {
		t.Fatalf("global host = %q; want 192.0.2.1", got)
	}
	if got := ResolveUIAllowedOrigins(nil, nil); !reflect.DeepEqual(got, []string{"https://global.test"}) {
		t.Fatalf("global origins = %#v; want global origin", got)
	}

	project := ConfigMap{"ui": map[string]any{
		"host":            "127.0.0.1",
		"allowed_origins": "https://one.test, https://two.test,https://one.test",
	}}
	if got := ResolveUIHost(nil, project); got != "127.0.0.1" {
		t.Fatalf("project host = %q; want 127.0.0.1", got)
	}
	wantOrigins := []string{"https://one.test", "https://two.test"}
	if got := ResolveUIAllowedOrigins(nil, project); !reflect.DeepEqual(got, wantOrigins) {
		t.Fatalf("origins = %#v; want %#v", got, wantOrigins)
	}
}

func TestGlobalS3CredentialsAreCompleteOrRemoved(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if err := SetGlobalS3Credentials("access", "secret"); err != nil {
		t.Fatalf("SetGlobalS3Credentials: %v", err)
	}
	cfg := HubS3Config()
	if !cfg.HasStaticCredentials() || cfg.AccessKeyID != "access" || cfg.SecretAccessKey != "secret" {
		t.Fatalf("stored credentials = %#v", cfg)
	}

	if err := SetGlobalS3Credentials("access", ""); err != nil {
		t.Fatalf("clearing partial credentials: %v", err)
	}
	cfg = HubS3Config()
	if cfg.AccessKeyID != "" || cfg.SecretAccessKey != "" || cfg.HasStaticCredentials() {
		t.Fatalf("partial credentials were retained: %#v", cfg)
	}
}

func TestS3ConfigRequiresACompleteStaticPair(t *testing.T) {
	if (S3Config{AccessKeyID: "access"}).HasStaticCredentials() {
		t.Fatal("access key without secret reported a complete static pair")
	}
	if !(S3Config{AccessKeyID: "access", SecretAccessKey: "secret"}).HasStaticCredentials() {
		t.Fatal("complete static pair was not detected")
	}
}
