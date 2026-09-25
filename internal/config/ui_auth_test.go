package config

import "testing"

func TestUIAuthConfigurationUsesLayeredResolution(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	project := ConfigMap{}
	SetConfigValue(project, "ui.auth.enabled", "true")
	SetConfigValue(project, "ui.auth.cookie_secure", "false")
	SetConfigValue(project, "ui.auth.public_url", "http://127.0.0.1:8080/")
	SetConfigValue(project, "ui.auth.cookie_encryption_key", "project-key")
	if !ResolveUIAuthEnabled(nil, project) || ResolveUIAuthCookieSecure(nil, project) || ResolveUIAuthPublicURL(nil, project) != "http://127.0.0.1:8080" {
		t.Fatal("project config was not resolved")
	}
	if ResolveUIAuthCookieEncryptionKey(nil, project) != "project-key" {
		t.Fatal("project cookie key was not resolved")
	}
	t.Setenv("GRAPHIT_UI_AUTH_ENABLED", "false")
	t.Setenv("GRAPHIT_UI_AUTH_COOKIE_SECURE", "true")
	t.Setenv("GRAPHIT_UI_AUTH_COOKIE_ENCRYPTION_KEY", "env-key")
	if ResolveUIAuthEnabled(nil, project) || !ResolveUIAuthCookieSecure(nil, project) {
		t.Fatal("environment config did not override project")
	}
	if ResolveUIAuthCookieEncryptionKey(nil, project) != "env-key" {
		t.Fatal("environment cookie key did not override project")
	}
	t.Setenv("GRAPHIT_UI_AUTH_COOKIE_ENCRYPTION_KEY", "")
	if err := SetGlobalConfigValue("ui.auth.cookie_encryption_key", "global-key"); err != nil {
		t.Fatal(err)
	}
	if ResolveUIAuthCookieEncryptionKey(nil, nil) != "global-key" || ConfigEnvVar("ui.auth.cookie_encryption_key") != "GRAPHIT_UI_AUTH_COOKIE_ENCRYPTION_KEY" {
		t.Fatal("global cookie key or environment mapping failed")
	}
	if err := SetGlobalConfigValue("ui.auth.enabled", "true"); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_UI_AUTH_ENABLED", "")
	if !ResolveUIAuthEnabled(nil, nil) || ConfigEnvVar("ui.auth.enabled") != "GRAPHIT_UI_AUTH_ENABLED" {
		t.Fatal("global config or env mapping failed")
	}
}
