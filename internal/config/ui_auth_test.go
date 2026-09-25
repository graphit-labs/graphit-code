package config

import "testing"

func TestUIAuthConfigurationUsesLayeredResolution(t *testing.T) {
	t.Setenv("GRAPHIT_GLOBAL_DIR", t.TempDir())
	project := ConfigMap{}
	SetConfigValue(project, "ui.auth.enabled", "true")
	SetConfigValue(project, "ui.auth.cookie_secure", "false")
	SetConfigValue(project, "ui.auth.public_url", "http://127.0.0.1:8080/")
	if !ResolveUIAuthEnabled(nil, project) || ResolveUIAuthCookieSecure(nil, project) || ResolveUIAuthPublicURL(nil, project) != "http://127.0.0.1:8080" {
		t.Fatal("project config was not resolved")
	}
	t.Setenv("GRAPHIT_UI_AUTH_ENABLED", "false")
	t.Setenv("GRAPHIT_UI_AUTH_COOKIE_SECURE", "true")
	if ResolveUIAuthEnabled(nil, project) || !ResolveUIAuthCookieSecure(nil, project) {
		t.Fatal("environment config did not override project")
	}
	if err := SetGlobalConfigValue("ui.auth.enabled", "true"); err != nil {
		t.Fatal(err)
	}
	t.Setenv("GRAPHIT_UI_AUTH_ENABLED", "")
	if !ResolveUIAuthEnabled(nil, nil) || ConfigEnvVar("ui.auth.enabled") != "GRAPHIT_UI_AUTH_ENABLED" {
		t.Fatal("global config or env mapping failed")
	}
}
