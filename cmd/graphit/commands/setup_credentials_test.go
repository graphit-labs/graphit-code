package commands

import (
	"bufio"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/output"
)

func TestPromptS3CredentialsStoresACompletePairAndClearsAPartialPair(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	if err := promptS3Credentials(p, bufio.NewReader(strings.NewReader("access\nsecret\n")), setupAnswers{}); err != nil {
		t.Fatalf("promptS3Credentials: %v", err)
	}
	cfg := config.HubS3Config()
	if cfg.AccessKeyID != "access" || cfg.SecretAccessKey != "secret" {
		t.Fatalf("stored credentials = %#v", cfg)
	}

	if err := promptS3Credentials(p, bufio.NewReader(strings.NewReader("access-only\n\n")), setupAnswers{}); err != nil {
		t.Fatalf("promptS3Credentials partial: %v", err)
	}
	cfg = config.HubS3Config()
	if cfg.AccessKeyID != "" || cfg.SecretAccessKey != "" {
		t.Fatalf("partial credentials were retained: %#v", cfg)
	}
}
