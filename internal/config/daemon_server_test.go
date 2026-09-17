package config

import (
	"reflect"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestResolveMCPAllowedOrigins(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(brand.EnvVar("MCP_ALLOWED_ORIGINS"), "")

	// Empty is the default, and it means the MCP endpoint emits no CORS headers at all.
	if got := ResolveMCPAllowedOrigins(nil, nil); got != nil {
		t.Fatalf("default origins = %#v; want none", got)
	}

	if err := SetGlobalConfigValue("mcp.allowed_origins", "https://global.test"); err != nil {
		t.Fatalf("set global MCP origins: %v", err)
	}
	if got := ResolveMCPAllowedOrigins(nil, nil); !reflect.DeepEqual(got, []string{"https://global.test"}) {
		t.Fatalf("global origins = %#v; want the configured origin", got)
	}

	// Same parsing as the UI listener: comma separated, trimmed, deduplicated in order.
	project := ConfigMap{"mcp": map[string]any{"allowed_origins": "https://one.test, https://two.test,https://one.test"}}
	want := []string{"https://one.test", "https://two.test"}
	if got := ResolveMCPAllowedOrigins(nil, project); !reflect.DeepEqual(got, want) {
		t.Fatalf("project origins = %#v; want %#v", got, want)
	}

	t.Setenv(brand.EnvVar("MCP_ALLOWED_ORIGINS"), "https://env.test")
	if got := ResolveMCPAllowedOrigins(nil, project); !reflect.DeepEqual(got, []string{"https://env.test"}) {
		t.Fatalf("environment origins = %#v; want the environment override", got)
	}
}
