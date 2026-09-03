package testenv

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

var home string

func init() {
	if !testing.Testing() {
		return
	}

	root := strings.TrimSpace(os.Getenv("GRAPHIT_TEST_HOME_ROOT"))
	if root == "" {
		root = filepath.Join(os.TempDir(), "graphit-test-homes")
	}
	if err := os.MkdirAll(root, 0o755); err != nil {
		panic("testenv: create home root: " + err.Error())
	}

	var err error
	home, err = os.MkdirTemp(root, "home-")
	if err != nil {
		panic("testenv: create home: " + err.Error())
	}

	for key, value := range map[string]string{
		"HOME":                      home,
		"USERPROFILE":               home,
		"XDG_CONFIG_HOME":           filepath.Join(home, ".config"),
		"XDG_CACHE_HOME":            filepath.Join(home, ".cache"),
		"XDG_DATA_HOME":             filepath.Join(home, ".local", "share"),
		"XDG_STATE_HOME":            filepath.Join(home, ".local", "state"),
		"GIT_CONFIG_GLOBAL":         filepath.Join(home, ".gitconfig"),
		"GIT_CONFIG_NOSYSTEM":       "1",
		"GIT_TERMINAL_PROMPT":       "0",
		"GIT_AUTHOR_NAME":           "Test",
		"GIT_AUTHOR_EMAIL":          "test@example.com",
		"GIT_COMMITTER_NAME":        "Test",
		"GIT_COMMITTER_EMAIL":       "test@example.com",
		"AWS_EC2_METADATA_DISABLED": "true",
		"HTTP_PROXY":                "http://127.0.0.1:1",
		"HTTPS_PROXY":               "http://127.0.0.1:1",
		"NO_PROXY":                  "127.0.0.1,localhost",
	} {
		_ = os.Setenv(key, value)
	}

	for _, key := range []string{
		"GRAPHIT_GLOBAL_DIR",
		"AWS_ACCESS_KEY_ID",
		"AWS_SECRET_ACCESS_KEY",
		"AWS_SESSION_TOKEN",
		"AWS_PROFILE",
		"AWS_SHARED_CREDENTIALS_FILE",
	} {
		_ = os.Unsetenv(key)
	}
}

func Home() string {
	return home
}

func Run(m *testing.M) int {
	code := m.Run()
	if home != "" {
		_ = os.RemoveAll(home)
	}
	return code
}
