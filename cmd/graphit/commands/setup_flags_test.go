package commands

import (
	"bufio"
	"strings"
	"testing"

	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

// failingReader stands in for stdin wherever a test asserts that a question was NOT asked. Any
// read is the failure itself: a fake that returned "" instead would let the test pass while
// setup silently fell through to a prompt and answered it with the default.
type failingReader struct{ t *testing.T }

func (r failingReader) Read([]byte) (int, error) {
	r.t.Helper()
	r.t.Fatal("stdin was read for a question whose flag was supplied")
	return 0, nil
}

func noPrompts(t *testing.T) *bufio.Reader {
	t.Helper()
	return bufio.NewReader(failingReader{t: t})
}

func answered(value string) setupAnswer { return setupAnswer{given: value, set: true} }

func TestSetupAnswersBindSeparatesUnsuppliedFromExplicitlyEmpty(t *testing.T) {
	var answers setupAnswers
	cmd := &cobra.Command{Use: "setup"}
	answers.register(cmd)

	if err := cmd.Flags().Parse([]string{"--ide", "cursor", "--hub-region", ""}); err != nil {
		t.Fatalf("Parse: %v", err)
	}
	answers.bind(cmd)

	if !answers.ide.set || answers.ide.given != "cursor" {
		t.Fatalf("ide answer = %#v, want {cursor true}", answers.ide)
	}
	if !answers.hubRegion.set || answers.hubRegion.given != "" {
		t.Fatalf("hub-region answer = %#v, want an explicitly empty value", answers.hubRegion)
	}
	if answers.hubBucket.set {
		t.Fatalf("hub-bucket was never passed but is marked as set: %#v", answers.hubBucket)
	}
}

// TestSetupCommandRegistersEveryAnswerFlag pins the flag surface to the answer struct. An answer
// nobody registered is unreachable from the command line, and the symptom would be a prompt
// appearing in a pipeline that believed it had answered everything.
func TestSetupCommandRegistersEveryAnswerFlag(t *testing.T) {
	cmd := newSetupCmd()

	var answers setupAnswers
	for name := range answers.fields() {
		if cmd.Flags().Lookup(name) == nil {
			t.Errorf("--%s is an answer field but is not registered as a flag", name)
		}
	}
}

// TestSetupHasNoNonInteractiveFlag pins the design: suppressing a question is done by answering
// it, never by a mode switch. A --non-interactive flag existed here briefly and was removed.
func TestSetupHasNoNonInteractiveFlag(t *testing.T) {
	if f := newSetupCmd().Flags().Lookup("non-interactive"); f != nil {
		t.Fatal("--non-interactive is back; a supplied flag is what suppresses its own question")
	}
}

func TestSetupAnswerValueAppliesTheFlagWithoutAsking(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := answered("flag-bucket").value(p, noPrompts(t),
		"hub bucket name", "hub.bucket", "config-bucket", "compiled-bucket", "blank hint")
	if err != nil {
		t.Fatalf("value: %v", err)
	}
	if got != "flag-bucket" {
		t.Fatalf("resolved = %q, want flag-bucket", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("hub.bucket"); v != "flag-bucket" {
		t.Fatalf("hub.bucket stored = %q, want flag-bucket", v)
	}
}

func TestSetupAnswerValueTrimsTheFlag(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := answered("  us-east-1  ").value(p, noPrompts(t),
		"bucket region", "hub.region", "", "", "blank hint")
	if err != nil {
		t.Fatalf("value: %v", err)
	}
	if got != "us-east-1" {
		t.Fatalf("resolved = %q, want us-east-1", got)
	}
}

func TestSetupAnswerValueClearsOnAnExplicitlyEmptyFlag(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	if err := config.SetGlobalConfigValue("hub.endpoint", "https://minio.example.test"); err != nil {
		t.Fatalf("seeding hub.endpoint: %v", err)
	}

	got, err := answered("").value(p, noPrompts(t),
		"S3 endpoint", "hub.endpoint", "https://minio.example.test", "", "blank hint")
	if err != nil {
		t.Fatalf("value: %v", err)
	}
	if got != "" {
		t.Fatalf("resolved = %q, want the empty string", got)
	}
	if v, ok, _ := config.GetGlobalConfigValue("hub.endpoint"); ok && v != "" {
		t.Fatalf("hub.endpoint should have been cleared, got %q", v)
	}
}

// An unsupplied answer must still ask. This is the half of the contract that a container build
// never exercises, and it is the half that would break every human install if it regressed.
func TestSetupAnswerValueStillPromptsWhenNoFlagWasSupplied(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := setupAnswer{}.value(p, bufio.NewReader(strings.NewReader("typed-bucket\n")),
		"hub bucket name", "hub.bucket", "", "", "blank hint")
	if err != nil {
		t.Fatalf("value: %v", err)
	}
	if got != "typed-bucket" {
		t.Fatalf("resolved = %q, want the typed answer", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("hub.bucket"); v != "typed-bucket" {
		t.Fatalf("hub.bucket stored = %q, want typed-bucket", v)
	}
}

func TestSetupAnswerSimpleKeepsCurrentOnAnEmptyFlagAndPromptsWithoutOne(t *testing.T) {
	if got := answered(" cursor ").simple(noPrompts(t), "default IDE", "claude"); got != "cursor" {
		t.Fatalf("simple = %q, want the trimmed flag value", got)
	}
	if got := answered("").simple(noPrompts(t), "default CLI", "claude"); got != "claude" {
		t.Fatalf("an explicitly empty --cli must keep the current value, got %q", got)
	}
	if got := (setupAnswer{}).simple(bufio.NewReader(strings.NewReader("codex\n")), "default IDE", "claude"); got != "codex" {
		t.Fatalf("simple = %q, want the typed answer", got)
	}
}

func TestSetupAnswerSecretTakesTheFlagWithoutAsking(t *testing.T) {
	p := output.NewPrinter("")

	got, err := answered("  sk-flag  ").secret(p, noPrompts(t), "API key", "hint")
	if err != nil {
		t.Fatalf("secret: %v", err)
	}
	if got != "sk-flag" {
		t.Fatalf("secret = %q, want sk-flag", got)
	}

	blank, err := answered("").secret(p, noPrompts(t), "API key", "hint")
	if err != nil {
		t.Fatalf("secret: %v", err)
	}
	if blank != "" {
		t.Fatalf("an explicitly empty key flag must store nothing, got %q", blank)
	}
}

func TestPromptS3CredentialsStoresTheFlagPairWithoutPrompting(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	answers := setupAnswers{
		accessKeyID: answered("flag-access"),
		secretKey:   answered("flag-secret"),
	}
	if err := promptS3Credentials(p, noPrompts(t), answers); err != nil {
		t.Fatalf("promptS3Credentials: %v", err)
	}

	cfg := config.HubS3Config()
	if cfg.AccessKeyID != "flag-access" || cfg.SecretAccessKey != "flag-secret" {
		t.Fatalf("stored credentials = %#v", cfg)
	}
}

// Supplying one half of the pair is not supplying the pair, so the other half is still asked
// about. The alternative — inferring the missing half as empty — would clear a stored credential
// on the strength of a flag the operator never wrote.
func TestPromptS3CredentialsAsksForTheHalfThatWasNotSupplied(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	answers := setupAnswers{accessKeyID: answered("flag-access")}
	if err := promptS3Credentials(p, bufio.NewReader(strings.NewReader("typed-secret\n")), answers); err != nil {
		t.Fatalf("promptS3Credentials: %v", err)
	}

	cfg := config.HubS3Config()
	if cfg.AccessKeyID != "flag-access" || cfg.SecretAccessKey != "typed-secret" {
		t.Fatalf("stored credentials = %#v", cfg)
	}
}

func TestPromptS3CredentialsClearsThePairWhenOneFlagIsExplicitlyEmpty(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	if err := config.SetGlobalS3Credentials("stored-access", "stored-secret"); err != nil {
		t.Fatalf("seeding credentials: %v", err)
	}

	answers := setupAnswers{accessKeyID: answered(""), secretKey: answered("")}
	if err := promptS3Credentials(p, noPrompts(t), answers); err != nil {
		t.Fatalf("promptS3Credentials: %v", err)
	}

	cfg := config.HubS3Config()
	if cfg.AccessKeyID != "" || cfg.SecretAccessKey != "" {
		t.Fatalf("a cleared pair was retained: %#v", cfg)
	}
}

func TestPromptEmbeddingProviderTakesTheFlagsWithoutPrompting(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	answers := setupAnswers{
		embeddingProvider: answered("openai-compatible"),
		embeddingBaseURL:  answered("http://localhost:11434/v1"),
		embeddingModel:    answered("nomic-embed-text"),
		embeddingAPIKey:   answered(""),
	}

	got, err := promptEmbeddingProvider(p, noPrompts(t), answers)
	if err != nil {
		t.Fatalf("promptEmbeddingProvider: %v", err)
	}
	if got != "openai-compatible" {
		t.Fatalf("provider = %q, want openai-compatible", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.embedding.base_url"); v != "http://localhost:11434/v1" {
		t.Fatalf("ai.embedding.base_url = %q", v)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.embedding.model"); v != "nomic-embed-text" {
		t.Fatalf("ai.embedding.model = %q", v)
	}
	if v, ok, _ := config.GetGlobalConfigValue("ai.embedding.api_key"); ok && v != "" {
		t.Fatalf("an explicitly empty --embedding-api-key stored a key anyway: %q", v)
	}
}

// --embedding-provider local is the whole answer: there is no model, base URL or key to ask
// about afterwards, which is what makes a container build reach zero prompts with one flag.
func TestPromptEmbeddingProviderLocalAsksNothingFurther(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := promptEmbeddingProvider(p, noPrompts(t), setupAnswers{embeddingProvider: answered("local")})
	if err != nil {
		t.Fatalf("promptEmbeddingProvider: %v", err)
	}
	if got != "local" {
		t.Fatalf("provider = %q, want local", got)
	}
}

func TestPromptEmbeddingProviderStillRejectsOpenAICompatibleWithoutABaseURL(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	// The model is answered too, and deliberately: without it this test would fail on the model
	// prompt instead of on the missing base URL, which is the semantics working — a provider that
	// is not `local` reaches three more questions, and every one of them needs its flag before the
	// run is silent.
	answers := setupAnswers{
		embeddingProvider: answered("openai-compatible"),
		embeddingModel:    answered(""),
		embeddingBaseURL:  answered(""),
	}
	if _, err := promptEmbeddingProvider(p, noPrompts(t), answers); err == nil {
		t.Fatal("expected an error for openai-compatible with no base URL")
	}
}

func TestPromptRerankProviderTakesTheFlagsWithoutPrompting(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	answers := setupAnswers{
		rerankProvider: answered("cohere"),
		rerankModel:    answered("rerank-english-v3.0"),
		rerankAPIKey:   answered(""),
	}
	got, err := promptRerankProvider(p, noPrompts(t), answers)
	if err != nil {
		t.Fatalf("promptRerankProvider: %v", err)
	}
	if got != "cohere" {
		t.Fatalf("provider = %q, want cohere", got)
	}
	if v, _, _ := config.GetGlobalConfigValue("ai.rerank.model"); v != "rerank-english-v3.0" {
		t.Fatalf("ai.rerank.model = %q", v)
	}
	if v, ok, _ := config.GetGlobalConfigValue("ai.rerank.api_key"); ok && v != "" {
		t.Fatalf("an explicitly empty --rerank-api-key stored a key anyway: %q", v)
	}
}

func TestPromptRerankProviderLocalAsksNothingFurther(t *testing.T) {
	t.Setenv(brand.EnvVar("GLOBAL_DIR"), t.TempDir())
	p := output.NewPrinter("")

	got, err := promptRerankProvider(p, noPrompts(t), setupAnswers{rerankProvider: answered("local")})
	if err != nil {
		t.Fatalf("promptRerankProvider: %v", err)
	}
	if got != "local" {
		t.Fatalf("provider = %q, want local", got)
	}
}
