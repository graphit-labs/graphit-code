package ast

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"sync"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/version"
	"github.com/hashicorp/go-plugin"
)

var (
	pluginClient *plugin.Client
	rpcParser    Parser
	pluginMu     sync.Mutex
)

func getParserPlugin(projectDir string) (Parser, error) {
	pluginMu.Lock()
	defer pluginMu.Unlock()

	if rpcParser != nil {
		return rpcParser, nil
	}

	runtimeDir := brand.RuntimeDir(version.Version)
	binName := "graphit-parser-plugin"
	if runtime.GOOS == "windows" {
		binName += ".exe"
	}
	pluginPath := filepath.Join(runtimeDir, binName)

	if _, err := os.Stat(pluginPath); os.IsNotExist(err) {
		if execPath, err := os.Executable(); err == nil {
			pluginPath = filepath.Join(filepath.Dir(execPath), binName)
		}
	}
	if _, err := os.Stat(pluginPath); os.IsNotExist(err) {
		pluginPath = filepath.Join(projectDir, ".build", binName)
	}

	if _, err := os.Stat(pluginPath); os.IsNotExist(err) {
		if path, err := exec.LookPath(binName); err == nil {
			pluginPath = path
		}
	}

	client := plugin.NewClient(&plugin.ClientConfig{
		HandshakeConfig: HandshakeConfig,
		Plugins: map[string]plugin.Plugin{
			"parser": &ParserPlugin{},
		},
		Cmd:     exec.Command(pluginPath),
		Managed: true,
	})

	rpcClient, err := client.Client()
	if err != nil {
		client.Kill()
		return nil, fmt.Errorf("failed to connect to parser plugin: %w", err)
	}

	raw, err := rpcClient.Dispense("parser")
	if err != nil {
		client.Kill()
		return nil, fmt.Errorf("failed to dispense parser plugin: %w", err)
	}

	pluginClient = client
	rpcParser = raw.(Parser)
	return rpcParser, nil
}

func CloseParserPlugin() {
	pluginMu.Lock()
	defer pluginMu.Unlock()
	if pluginClient != nil {
		pluginClient.Kill()
		pluginClient = nil
		rpcParser = nil
	}
}


