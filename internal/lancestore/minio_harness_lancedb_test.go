//go:build lancedb

package lancestore

import (
	"context"
	"fmt"
	"net"
	"os/exec"
	"strings"
	"testing"
	"time"

	"github.com/graphit-labs/graphit-code/internal/config"
)

const (
	minioImage     = "minio/minio:latest"
	minioAccessKey = "graphittest"
	minioSecretKey = "graphittest"
	minioRegion    = "us-east-1"
	minioReadyWait = 30 * time.Second
)

func minioBucket(t *testing.T) (bucket string, s3 config.S3Config) {
	t.Helper()
	requireDocker(t)

	port := freePort(t)
	name := fmt.Sprintf("graphit-lancestore-test-%d", port)

	run := exec.Command("docker", "run", "--rm", "-d",
		"--name", name,
		"-p", fmt.Sprintf("127.0.0.1:%d:9000", port),
		"-e", "MINIO_ROOT_USER="+minioAccessKey,
		"-e", "MINIO_ROOT_PASSWORD="+minioSecretKey,
		minioImage, "server", "/data")
	out, err := run.CombinedOutput()
	if err != nil {
		t.Skipf("cannot start MinIO (%v): %s", err, strings.TrimSpace(string(out)))
	}
	t.Cleanup(func() {
		_ = exec.Command("docker", "rm", "-f", name).Run()
	})

	endpoint := fmt.Sprintf("http://127.0.0.1:%d", port)
	waitForMinIO(t, name, endpoint)

	bucket = "lancestore"
	mc := exec.Command("docker", "exec", name, "mc", "alias", "set", "local",
		"http://127.0.0.1:9000", minioAccessKey, minioSecretKey)
	if out, err := mc.CombinedOutput(); err != nil {
		t.Skipf("cannot configure the MinIO client (%v): %s", err, strings.TrimSpace(string(out)))
	}
	mb := exec.Command("docker", "exec", name, "mc", "mb", "local/"+bucket)
	if out, err := mb.CombinedOutput(); err != nil && !strings.Contains(string(out), "already own it") {
		t.Skipf("cannot create the bucket (%v): %s", err, strings.TrimSpace(string(out)))
	}

	return bucket, config.S3Config{
		Bucket:          bucket,
		Region:          minioRegion,
		Endpoint:        endpoint,
		AccessKeyID:     minioAccessKey,
		SecretAccessKey: minioSecretKey,
	}
}

func remoteConfig(t *testing.T, prefix string) Config {
	t.Helper()
	bucket, s3 := minioBucket(t)
	return Config{
		URI:      fmt.Sprintf("s3://%s/%s", bucket, prefix),
		S3:       s3,
		Writable: true,
	}
}

func requireDocker(t *testing.T) {
	t.Helper()
	if _, err := exec.LookPath("docker"); err != nil {
		t.Skip("docker is not installed; the remote-store proof needs a real S3")
	}
	if err := exec.Command("docker", "info").Run(); err != nil {
		t.Skip("the docker daemon is not reachable; the remote-store proof needs a real S3")
	}
}

func waitForMinIO(t *testing.T, container, endpoint string) {
	t.Helper()
	addr := strings.TrimPrefix(endpoint, "http://")
	deadline := time.Now().Add(minioReadyWait)
	for time.Now().Before(deadline) {
		conn, err := net.DialTimeout("tcp", addr, time.Second)
		if err == nil {
			_ = conn.Close()
			time.Sleep(500 * time.Millisecond)
			return
		}
		time.Sleep(200 * time.Millisecond)
	}
	logs, _ := exec.Command("docker", "logs", "--tail", "20", container).CombinedOutput()
	t.Skipf("MinIO did not become ready within %s; logs:\n%s", minioReadyWait, strings.TrimSpace(string(logs)))
}

func freePort(t *testing.T) int {
	t.Helper()
	l, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("reserving a port: %v", err)
	}
	defer func() { _ = l.Close() }()
	return l.Addr().(*net.TCPAddr).Port
}

func remoteTable(t *testing.T, prefix string) (*Store, *Table) {
	t.Helper()
	ctx := context.Background()
	st, err := Open(ctx, remoteConfig(t, prefix))
	if err != nil {
		t.Fatalf("opening the remote store: %v", err)
	}
	t.Cleanup(func() { _ = st.Close() })

	tbl, err := st.CreateTable(ctx, "memories", testSchema())
	if err != nil {
		t.Fatalf("creating a table on s3: %v", err)
	}
	return st, tbl
}
