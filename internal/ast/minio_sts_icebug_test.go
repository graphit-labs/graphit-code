package ast

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/ladybugstore"
	"github.com/graphit-labs/graphit-code/internal/s3store"
)

type minioSTSIcebugCredentials struct {
	AccessKeyID     string    `json:"access_key_id"`
	SecretAccessKey string    `json:"secret_access_key"`
	SessionToken    string    `json:"session_token"`
	ExpiresAt       time.Time `json:"expires_at"`
	Bucket          string    `json:"bucket"`
	Region          string    `json:"region"`
	Endpoint        string    `json:"endpoint"`
	Prefixes        []string  `json:"prefixes"`
}

func TestMinIOSTSIcebugQueriesRemoteParquet(t *testing.T) {
	credentialFile := os.Getenv("GRAPHIT_MINIO_STS_CREDENTIAL_FILE")
	if credentialFile == "" {
		t.Skip("GRAPHIT_MINIO_STS_CREDENTIAL_FILE is not set")
	}
	raw, err := os.ReadFile(credentialFile)
	if err != nil {
		t.Fatal(err)
	}
	var temporary minioSTSIcebugCredentials
	if err := json.Unmarshal(raw, &temporary); err != nil {
		t.Fatal(err)
	}
	if temporary.SessionToken == "" || !temporary.ExpiresAt.After(time.Now()) || len(temporary.Prefixes) != 1 {
		t.Fatal("credential file does not contain a live STS session and root")
	}
	extensionSource := os.Getenv("GRAPHIT_LBUG_HTTPFS_EXTENSION")
	if extensionSource == "" {
		extensionSource = ladybugstore.ExtensionPath(ladybugstore.ExtHTTPFS)
	}
	extension, err := os.ReadFile(extensionSource)
	if err != nil {
		t.Fatalf("read installed httpfs extension: %v", err)
	}

	home := t.TempDir()
	t.Setenv("HOME", home)
	t.Setenv("USERPROFILE", home)
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), filepath.Join(home, brand.DotDir()))
	extensionPath := ladybugstore.ExtensionPath(ladybugstore.ExtHTTPFS)
	if err := os.MkdirAll(filepath.Dir(extensionPath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(extensionPath, extension, 0o755); err != nil {
		t.Fatal(err)
	}
	provider := auth.Provider{
		Name: "minio", Type: auth.ProviderLocal, Local: &auth.LocalConfig{},
		S3: auth.S3Config{Bucket: temporary.Bucket, Region: temporary.Region, Endpoint: temporary.Endpoint, Prefix: temporary.Prefixes[0]},
	}
	authStore, err := auth.Open()
	if err != nil {
		t.Fatal(err)
	}
	if err := authStore.AddProvider(provider); err != nil {
		t.Fatal(err)
	}
	if err := authStore.Login(auth.Profile{
		Name: "alice", Provider: "minio", Username: "alice",
		S3: auth.S3Credentials{
			AccessKeyID: temporary.AccessKeyID, SecretAccessKey: temporary.SecretAccessKey,
			SessionToken: temporary.SessionToken, ExpiresAt: temporary.ExpiresAt,
			Prefixes: temporary.Prefixes,
		},
	}); err != nil {
		t.Fatal(err)
	}

	ctx := context.Background()
	prefix := temporary.Prefixes[0] + "/v2/projects/project-a/integration/icebug-sts"
	remoteURI := "s3://" + temporary.Bucket + "/" + prefix
	entry := &parseCacheEntry{RelPath: "f.go", Language: "go", Entities: []cachedEntity{
		{Label: "Function", UID: "fn_remote", Name: "remote", Path: "f.go", Line: 1, EndLine: 2},
	}}
	ri := newRebuildIndex(map[string]*parseCacheEntry{"f.go": entry}, targetRulesFor(""))
	bundle := t.TempDir()
	if _, err := ExportDirectFromRebuildIndex(ri, bundle, remoteURI); err != nil {
		t.Fatal(err)
	}
	cfg := config.S3Config{
		Bucket: temporary.Bucket, Region: temporary.Region, Endpoint: temporary.Endpoint,
		AccessKeyID: temporary.AccessKeyID, SecretAccessKey: temporary.SecretAccessKey,
		SessionToken: temporary.SessionToken, ExpiresAt: temporary.ExpiresAt,
	}
	objects, err := s3store.New(ctx, cfg)
	if err != nil {
		t.Fatal(err)
	}
	if err := objects.UploadDir(ctx, bundle, prefix); err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = objects.DeletePrefix(context.Background(), prefix) })
	if _, err := objects.Get(ctx, prefix+"/nodes_Function.parquet"); err != nil {
		t.Fatalf("uploaded remote Parquet is not readable through the SDK: %v", err)
	}

	mountDir := t.TempDir()
	for _, name := range []string{"schema.cypher", ladybugstore.IcebugManifestFile} {
		data, readErr := os.ReadFile(filepath.Join(bundle, name))
		if readErr != nil {
			t.Fatal(readErr)
		}
		if writeErr := os.WriteFile(filepath.Join(mountDir, name), data, 0o644); writeErr != nil {
			t.Fatal(writeErr)
		}
	}
	db := NewLadybugDBReadOnly(LadybugConfig{StoreDir: mountDir, IcebugDir: mountDir})
	defer db.Close()
	result, err := db.Query(ctx, "MATCH (n:Function) RETURN n.uid AS uid", nil)
	if err != nil {
		t.Fatal(err)
	}
	if got := recordStrings(result, "uid"); len(got) != 1 || got[0] != "fn_remote" {
		t.Fatalf("remote Icebug query = %v", got)
	}
}
