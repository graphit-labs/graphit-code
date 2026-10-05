package config

import (
	"os"
	"strconv"
	"strings"
)

const (
	// DefaultMCPHost keeps the MCP listener on the loopback interface. The daemon's MCP endpoint
	// is authenticated by a bearer key, but a key is not a reason to publish a port: the stdio
	// proxy that every Agent uses reaches it over loopback, so nothing needs it exposed. A container
	// that must publish it says so explicitly with `mcp.host`.
	DefaultMCPHost = "127.0.0.1"

	// DefaultMCPPort is 0, meaning the kernel picks a free port — which is what the daemon did
	// before this key existed, and what lets several daemons coexist on a workstation. The chosen
	// port is published to <DaemonDir>/mcp.port for the proxy to read.
	//
	// A container needs the opposite: a port known before the process starts, so it can be
	// declared in the image and mapped on the host. Setting `mcp.port` pins it.
	DefaultMCPPort = 0
)

// ResolveMCPHost returns the interface the daemon's MCP server binds to.
func ResolveMCPHost(inlineCfg, projectCfg ConfigMap) string {
	host := strings.TrimSpace(ResolveConfig("mcp.host", inlineCfg, projectCfg))
	if host == "" {
		return DefaultMCPHost
	}
	return host
}

// ResolveMCPPort returns the port the daemon's MCP server binds to, or 0 for an
// OS-assigned one.
//
// A value that is not a valid port falls back to 0 rather than failing the daemon: an
// unparseable port is a configuration mistake, and refusing to start the daemon over it would
// take the MCP server, the indexers and — in a container — PID 1 down with it. The published
// port file still tells anyone which port was actually used.
func ResolveMCPPort(inlineCfg, projectCfg ConfigMap) int {
	raw := strings.TrimSpace(ResolveConfig("mcp.port", inlineCfg, projectCfg))
	if raw == "" {
		return DefaultMCPPort
	}
	port, err := strconv.Atoi(raw)
	if err != nil || port < 0 || port > 65535 {
		return DefaultMCPPort
	}
	return port
}

// ResolveMCPAllowedOrigins returns the browser origins allowed to call the daemon's MCP
// endpoint, mirroring ui.allowed_origins for the other listener.
//
// Empty is the default and means no CORS headers at all, which is what the endpoint did
// before this key existed. A browser-based MCP client is the only caller that needs it;
// an agent whose runtime fetches server-side is unaffected either way. Declaring an origin
// is therefore a deliberate act, never something a deployment inherits by accident.
func ResolveMCPAllowedOrigins(inlineCfg, projectCfg ConfigMap) []string {
	return splitAllowedOrigins(ResolveConfig("mcp.allowed_origins", inlineCfg, projectCfg))
}

// DaemonUIModule names the environment opt-in for starting the UI with the daemon.
// Without it, the tray or `ui` command activates the same supervised server later.
const DaemonUIModule = "daemon_ui"

// DaemonUIOnStartup deliberately ignores stored project/global configuration.
// Only the explicit environment opt-in allocates UI resources during startup.
func DaemonUIOnStartup() bool {
	enabled, err := strconv.ParseBool(strings.TrimSpace(os.Getenv(ConfigEnvVar("modules." + DaemonUIModule))))
	return err == nil && enabled
}
