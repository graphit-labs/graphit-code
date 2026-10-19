//! Shared concrete bounds for the existing CLI protocol and Process adapters.
use graphit_ai::{cli_codex_wire::CodexWireLimits, codex_completion::CompletionLimits};
use graphit_process::Limits;
use std::time::Duration;

pub(crate) struct CliLaunchPolicy {
    pub wire: CodexWireLimits,
    pub completion: CompletionLimits,
    pub process: Limits,
}
impl CliLaunchPolicy {
    pub fn capture() -> Self {
        Self {
            wire: CodexWireLimits {
                frame_bytes: 1024 * 1024,
                depth: 64,
                nodes: 65536,
                fields: 65536,
                array_items: 65536,
                string_bytes: 1024 * 1024,
                decoded_string_bytes: 1024 * 1024,
                id_bytes: 4096,
            },
            completion: CompletionLimits {
                max_graphit_attempts: 1,
                max_response_bytes: 8 * 1024 * 1024,
                max_model_id_bytes: 4096,
            },
            process: Limits {
                stdin_bytes: 1024 * 1024,
                stdout_bytes: 8 * 1024 * 1024,
                stderr_bytes: 64 * 1024,
                termination_grace: Duration::from_secs(2),
                cleanup: Duration::from_secs(10),
            },
        }
    }
    pub fn kimi_limits(
        &self,
        prompt_text_bytes: usize,
        response_text_bytes: usize,
    ) -> graphit_ai::cli_kimi::KimiLimits {
        let positive = |value| std::num::NonZeroUsize::new(value).expect("captured positive limit");
        graphit_ai::cli_kimi::KimiLimits {
            wire: self.wire,
            network: graphit_http::client::Limits {
                request_body_bytes: self.wire.frame_bytes,
                response_body_bytes: self.wire.frame_bytes,
                url_bytes: self.wire.id_bytes,
                header_bytes: self.wire.id_bytes,
                header_count: 16,
                max_in_flight: 1,
                max_idle_per_host: 1,
                idle_timeout: self.process.cleanup,
                max_certificates: 1,
                max_certificate_bytes: self.wire.frame_bytes,
            },
            websocket: graphit_http::websocket::WebSocketLimits {
                frame_bytes: positive(self.wire.frame_bytes),
                message_bytes: positive(self.wire.frame_bytes),
                send_bytes: positive(self.wire.frame_bytes),
            },
            prompt_text_bytes,
            response_text_bytes,
        }
    }
}
