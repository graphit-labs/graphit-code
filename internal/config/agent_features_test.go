package config

import (
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
)

func TestAgentFeaturesAreEnabledByDefault(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if !AgentFeaturesEnabled(nil, nil) {
		t.Fatal("agent features must be on by default: a machine with a CLI installed should need no configuration")
	}
	if isOptInModule(AgentModule) {
		t.Fatalf("%q must not be opt-in", AgentModule)
	}
}

func TestAgentFeaturesDisabledByConfigAndByEnvironment(t *testing.T) {
	t.Run("global config", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
		if err := SetGlobalConfigValue("modules."+AgentModule, "false"); err != nil {
			t.Fatalf("SetGlobalConfigValue: %v", err)
		}
		if AgentFeaturesEnabled(nil, nil) {
			t.Fatal("modules.agent=false must disable agent features")
		}
	})

	// The environment variable is what a container uses, and it must reach this without the image
	// writing a config file. The name is not declared anywhere — ConfigEnvVar derives it.
	t.Run("environment variable", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
		t.Setenv(ConfigEnvVar("modules."+AgentModule), "false")
		if AgentFeaturesEnabled(nil, nil) {
			t.Fatalf("%s=false must disable agent features", ConfigEnvVar("modules."+AgentModule))
		}
	})

	// A project may re-enable what the machine turned off, because the module flag is an ordinary
	// config key and project outranks global.
	t.Run("project config outranks global", func(t *testing.T) {
		t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
		if err := SetGlobalConfigValue("modules."+AgentModule, "false"); err != nil {
			t.Fatalf("SetGlobalConfigValue: %v", err)
		}
		projectCfg := ConfigMap{}
		SetConfigValue(projectCfg, "modules."+AgentModule, "true")
		if !AgentFeaturesEnabled(nil, projectCfg) {
			t.Fatal("a project setting modules.agent=true must win over a global false")
		}
	})
}

func TestDaemonUIIsOptIn(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if DaemonServesUI(nil, nil) {
		t.Fatal("the daemon must not serve a UI unless asked: it would hold a port nobody requested")
	}
	if !isOptInModule(DaemonUIModule) {
		t.Fatalf("%q must be in OptInModules", DaemonUIModule)
	}

	t.Setenv(ConfigEnvVar("modules."+DaemonUIModule), "true")
	if !DaemonServesUI(nil, nil) {
		t.Fatalf("%s=true must make the daemon serve the UI", ConfigEnvVar("modules."+DaemonUIModule))
	}
}

func TestMCPHostAndPortDefaultToThePreviousBehaviour(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())

	if got := ResolveMCPHost(nil, nil); got != DefaultMCPHost {
		t.Fatalf("ResolveMCPHost = %q, want %q — the default must stay loopback", got, DefaultMCPHost)
	}
	if got := ResolveMCPPort(nil, nil); got != 0 {
		t.Fatalf("ResolveMCPPort = %d, want 0 — an OS-assigned port is the pre-existing behaviour", got)
	}
}

func TestMCPHostAndPortFromConfiguration(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	t.Setenv(ConfigEnvVar("mcp.host"), "0.0.0.0")
	t.Setenv(ConfigEnvVar("mcp.port"), "8081")

	if got := ResolveMCPHost(nil, nil); got != "0.0.0.0" {
		t.Fatalf("ResolveMCPHost = %q, want 0.0.0.0", got)
	}
	if got := ResolveMCPPort(nil, nil); got != 8081 {
		t.Fatalf("ResolveMCPPort = %d, want 8081", got)
	}
}

// An unparseable or out-of-range port must not take the daemon down with it — in a container the
// daemon is PID 1, so refusing to start over a typo would be an outage rather than a diagnostic.
func TestMCPPortFallsBackRatherThanFailing(t *testing.T) {
	for _, bad := range []string{"not-a-port", "-1", "70000", "8080.5"} {
		t.Run(bad, func(t *testing.T) {
			t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
			t.Setenv(ConfigEnvVar("mcp.port"), bad)
			if got := ResolveMCPPort(nil, nil); got != DefaultMCPPort {
				t.Fatalf("ResolveMCPPort(%q) = %d, want the default %d", bad, got, DefaultMCPPort)
			}
		})
	}
}
