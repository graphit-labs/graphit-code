package commands

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"strings"

	"github.com/graphit-labs/graphit-code/internal/ai"
	"github.com/graphit-labs/graphit-code/internal/auth"
	"github.com/graphit-labs/graphit-code/internal/brand"
	"github.com/graphit-labs/graphit-code/internal/config"
	"github.com/graphit-labs/graphit-code/internal/daemon"
	"github.com/graphit-labs/graphit-code/internal/memory"
	"github.com/graphit-labs/graphit-code/internal/output"
	"github.com/spf13/cobra"
)

func newSetupCmd() *cobra.Command {

	var answers setupAnswers

	cmd := &cobra.Command{
		Use:   "setup",
		Short: "Configure " + brand.DisplayName + " installation, runtime, Agent, and CLI",
		Long: `Configure installation and runtime settings for ` + brand.DisplayName + `.

Setup provisions the local installation identity, the persisted default local AI provider, and event privacy and Agent/CLI defaults. Customize AI topology, including local ONNX execution devices, with '` + brand.BinName() + ` provider add' or '` + brand.BinName() + ` provider update'; use '` + brand.BinName() + ` login' for account identity,
MCP credentials, S3 credentials, OIDC sessions, and STS exchange.

Every question has a flag. With --non-interactive every applicable flag must be supplied;
an explicit empty value selects a documented default without opening a prompt.`,
		SilenceUsage: true,
		RunE: func(cmd *cobra.Command, args []string) error {
			p := output.NewPrinter("")
			reader := bufio.NewReader(os.Stdin)
			answers.bind(cmd)
			if nonInteractive {
				if err := answers.validateNonInteractive(); err != nil {
					return err
				}
			}

			if _, err := exec.LookPath("git"); err != nil {
				p.Error("git is required but was not found in PATH")
				p.Detail("Install git", "https://git-scm.com/downloads")
				return fmt.Errorf("git CLI not found in PATH: %w", err)
			}

			p.Header("Welcome to %s setup", brand.DisplayName)
			if err := config.EnsureInstallationIdentity(); err != nil {
				return fmt.Errorf("initializing installation identity: %w", err)
			}
			p.StepOK("Installation identity ready")
			authStore, err := auth.Open()
			if err != nil {
				return fmt.Errorf("opening authentication state: %w", err)
			}
			if _, _, err := authStore.EnsureDefaultLocalProvider(); err != nil {
				return fmt.Errorf("initializing default local provider: %w", err)
			}
			p.StepOK("Default local AI provider ready")

			if _, err := promptEventAnonymization(p, reader, answers.anonymizeEvents); err != nil {
				return err
			}

			agentInput := answers.agent.simple(reader, "default Agent", config.DefaultAgent())
			if err := config.SetGlobalConfigValue("agent", agentInput); err != nil {
				return fmt.Errorf("saving agent: %w", err)
			}
			p.StepOK("Default Agent: %s", agentInput)

			cliInput := answers.cli.simple(reader, "default CLI", config.DefaultCLI())
			if err := config.SetGlobalConfigValue("cli", cliInput); err != nil {
				return fmt.Errorf("saving cli: %w", err)
			}
			p.StepOK("Default CLI: %s", cliInput)
			p.Blank()

			memTask := p.StartTask("Initialising memory store...")
			memStore, err := memory.NewMemoryStore()
			if err != nil {
				memTask.Fail("Memory store failed: %v", err)
				return fmt.Errorf("resolving memory store path: %w", err)
			}
			if err := memStore.EnsureInitialised(); err != nil {
				memTask.Fail("Memory store init: %v", err)
			} else {
				memTask.Done("Memory store ready at %s", memStore.Dir())
			}

			if err := ai.PrefetchConfiguredLocalModels(cmd.Context()); err != nil {
				return fmt.Errorf("provisioning configured local models: %w", err)
			}

			p.Blank()
			p.Success("Setup complete! Run '%s init' to initialize a project.", brand.BinName())

			if !config.IsModuleDisabled("daemon", nil, nil) {
				_, _ = daemon.EnsureRunning()
			}

			return nil
		},
	}

	answers.register(cmd)
	return cmd
}

type setupAnswer struct {
	given string
	set   bool
}

type setupAnswers struct {
	anonymizeEvents setupAnswer

	agent setupAnswer
	cli   setupAnswer
}

func (a *setupAnswers) fields() map[string]*setupAnswer {
	return map[string]*setupAnswer{
		"anonymize-events": &a.anonymizeEvents,
		"agent":            &a.agent,
		"cli":              &a.cli,
	}
}

func (a *setupAnswers) register(cmd *cobra.Command) {
	f := cmd.Flags()

	f.String("anonymize-events", "", "Anonymize project and user IDs in Hub event payloads [true/false] (default false)")
	f.Lookup("anonymize-events").NoOptDefVal = "true"

	f.String("agent", "", "Default Agent")
	f.String("cli", "", "Default CLI used for AI fallback")
}

func (a setupAnswers) validateNonInteractive() error {
	required := map[string]*setupAnswer{
		"anonymize-events": &a.anonymizeEvents,
		"agent":            &a.agent,
		"cli":              &a.cli,
	}
	for name, answer := range required {
		if !answer.set {
			return fmt.Errorf("--%s is required with --non-interactive (pass an explicit empty value when applicable)", name)
		}
	}
	return nil
}

func promptEventAnonymization(p *output.Printer, reader *bufio.Reader, answer setupAnswer) (bool, error) {
	if answer.set && strings.TrimSpace(answer.given) == "" {
		if err := config.UnsetGlobalConfigValue(config.HubEventsAnonymizeConfigKey); err != nil {
			return false, fmt.Errorf("clearing %s: %w", config.HubEventsAnonymizeConfigKey, err)
		}
		p.StepOK("Hub event identifiers: explicit (default)")
		return false, nil
	}
	enabled, err := answer.boolean(reader, "anonymize project and user IDs in Hub events", config.HubEventsAnonymize())
	if err != nil {
		return false, err
	}
	if err := config.SetGlobalConfigValue(config.HubEventsAnonymizeConfigKey, fmt.Sprintf("%t", enabled)); err != nil {
		return false, fmt.Errorf("saving %s: %w", config.HubEventsAnonymizeConfigKey, err)
	}
	if enabled {
		p.StepOK("Hub event identifiers: anonymized")
	} else {
		p.StepOK("Hub event identifiers: explicit")
	}
	return enabled, nil
}

func (a *setupAnswers) bind(cmd *cobra.Command) {
	for name, answer := range a.fields() {
		value, err := cmd.Flags().GetString(name)
		if err != nil {
			continue
		}
		answer.given = value
		answer.set = cmd.Flags().Changed(name)
	}
}

func (answer setupAnswer) simple(reader *bufio.Reader, label, current string) string {
	if !answer.set {
		return promptSimple(reader, label, current)
	}
	if trimmed := strings.TrimSpace(answer.given); trimmed != "" {
		return trimmed
	}
	return current
}

func (answer setupAnswer) boolean(reader *bufio.Reader, label string, current bool) (bool, error) {
	raw := answer.given
	if !answer.set {
		hint := "y/N"
		if current {
			hint = "Y/n"
		}
		fmt.Printf("  %s [%s]: ", label, hint)
		input, _ := reader.ReadString('\n')
		raw = input
		if strings.TrimSpace(raw) == "" {
			return current, nil
		}
	}

	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "true", "t", "1", "yes", "y":
		return true, nil
	case "false", "f", "0", "no", "n":
		return false, nil
	default:
		return false, fmt.Errorf("%s must be true or false", label)
	}
}

func promptSimple(reader *bufio.Reader, label, current string) string {
	fmt.Printf("  Enter %s [%s]: ", label, current)
	input, _ := reader.ReadString('\n')
	input = strings.TrimSpace(input)
	if input == "" {
		return current
	}
	return input
}
