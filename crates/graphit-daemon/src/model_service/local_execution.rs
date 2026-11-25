//! Canonical local execution selected by service purpose, before artifact capture.
use super::{
    ModelErrorKind, ModelResult, ModelServiceError,
    config::{ModelSettings, SlotSettings, TaskKind},
};
use graphit_ai::local_execution::LocalProvider;
use graphit_auth::profile::{AiService, LocalDevice, LocalExecution};
use std::io;

/// A copied selector, never an authorization, native-device discovery or fallback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LocalExecutionSelection {
    pub embedding: Option<LocalExecution>,
    pub rerank: Option<LocalExecution>,
}

impl LocalExecutionSelection {
    pub fn from_services(
        embedding: &AiService,
        rerank: &AiService,
        rerank_enabled: bool,
    ) -> ModelResult<Self> {
        let local = |service: &AiService| match service {
            AiService::Local(execution) => Some(*execution),
            AiService::Disabled | AiService::Broker | AiService::Direct { .. } => None,
        };
        let selected = Self {
            embedding: local(embedding),
            rerank: if rerank_enabled { local(rerank) } else { None },
        };
        selected.validate()?;
        Ok(selected)
    }

    pub fn has_local(&self) -> bool {
        self.embedding.is_some() || self.rerank.is_some()
    }

    pub(crate) fn for_task(self, task: TaskKind) -> Option<LocalExecution> {
        match task {
            TaskKind::Embedding => self.embedding,
            TaskKind::Rerank => self.rerank,
        }
    }

    pub(crate) fn validate(self) -> ModelResult<()> {
        for execution in [self.embedding, self.rerank].into_iter().flatten() {
            if execution.device_id > i32::MAX as u32 {
                return Err(ModelServiceError::caused(
                    ModelErrorKind::Configuration,
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "local model device index exceeds the runtime index range",
                    ),
                ));
            }
            if execution.device == LocalDevice::Cpu && execution.device_id != 0 {
                return Err(ModelServiceError::caused(
                    ModelErrorKind::Configuration,
                    io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "CPU execution requires device_id zero",
                    ),
                ));
            }
            execution
                .validate_platform(cfg!(target_os = "macos"))
                .map_err(|cause| ModelServiceError::caused(ModelErrorKind::Configuration, cause))?;
        }
        Ok(())
    }
}

pub(crate) fn local_provider(device: LocalDevice) -> LocalProvider {
    match device {
        LocalDevice::Auto => LocalProvider::Auto,
        LocalDevice::Cpu => LocalProvider::CPU,
        LocalDevice::Cuda => LocalProvider::CUDA,
        LocalDevice::Metal => LocalProvider::Metal,
    }
}

impl ModelSettings {
    /// Call before capturing model bundles: remote and disabled purposes need no local files.
    pub fn local_slots(
        &self,
        selection: LocalExecutionSelection,
    ) -> impl Iterator<Item = &SlotSettings> {
        self.slots
            .iter()
            .filter(move |slot| selection.for_task(slot.selection.task).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model_service::{RuntimeSettings, SelectionSettings};
    use graphit_auth::profile::DirectProtocol;
    use std::{num::NonZeroU32, num::NonZeroUsize, path::PathBuf};

    fn local(device: LocalDevice, device_id: u32) -> AiService {
        AiService::Local(LocalExecution { device, device_id })
    }

    #[test]
    fn cpu_selection_is_explicit_and_rerank_opt_in_is_independent() {
        let cpu = local(LocalDevice::Cpu, 0);
        let selected = LocalExecutionSelection::from_services(&cpu, &cpu, false).unwrap();
        assert_eq!(
            selected.embedding,
            Some(LocalExecution {
                device: LocalDevice::Cpu,
                device_id: 0,
            })
        );
        assert_eq!(selected.rerank, None);
        assert!(selected.has_local());
    }

    #[test]
    fn embedding_and_rerank_keep_independent_canonical_devices() {
        let selected = LocalExecutionSelection::from_services(
            &local(LocalDevice::Cuda, 2),
            &local(LocalDevice::Cpu, 0),
            true,
        )
        .unwrap();
        assert_eq!(selected.embedding.unwrap().device, LocalDevice::Cuda);
        assert_eq!(selected.embedding.unwrap().device_id, 2);
        assert_eq!(selected.rerank.unwrap().device, LocalDevice::Cpu);
        assert_eq!(selected.rerank.unwrap().device_id, 0);
    }

    #[test]
    fn inactive_rerank_does_not_validate_or_admit_its_local_execution() {
        let invalid_rerank = local(LocalDevice::Cpu, u32::MAX);
        let selected = LocalExecutionSelection::from_services(
            &local(LocalDevice::Cpu, 0),
            &invalid_rerank,
            false,
        )
        .unwrap();
        assert_eq!(selected.rerank, None);
        let error = LocalExecutionSelection::from_services(
            &local(LocalDevice::Cpu, 0),
            &invalid_rerank,
            true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), ModelErrorKind::Configuration);
    }

    #[test]
    fn direct_embedding_and_disabled_rerank_need_no_local_model() {
        let direct = AiService::Direct {
            protocol: DirectProtocol::OpenAi,
            endpoint: "https://example.invalid/embeddings".to_owned(),
            model: "configured-remote-model".to_owned(),
            model_revision: "revision".to_owned(),
            recipe: "remote-recipe".to_owned(),
            dimensions: 768,
            request_dimensions: None,
            batch_endpoint: None,
        };
        let selected =
            LocalExecutionSelection::from_services(&direct, &AiService::Disabled, false).unwrap();
        assert_eq!(selected.embedding, None);
        assert_eq!(selected.rerank, None);
        assert!(!selected.has_local());
    }

    #[test]
    fn broker_embedding_does_not_disable_opted_in_local_rerank() {
        let selected = LocalExecutionSelection::from_services(
            &AiService::Broker,
            &local(LocalDevice::Cuda, 1),
            true,
        )
        .unwrap();
        assert_eq!(selected.embedding, None);
        assert_eq!(selected.rerank.unwrap().device_id, 1);
        assert!(selected.has_local());
    }

    #[test]
    fn local_cpu_index_and_runtime_index_overflow_are_configuration_errors() {
        for execution in [
            local(LocalDevice::Cpu, 1),
            local(LocalDevice::Cuda, u32::MAX),
        ] {
            let error =
                LocalExecutionSelection::from_services(&execution, &AiService::Disabled, false)
                    .unwrap_err();
            assert_eq!(error.kind(), ModelErrorKind::Configuration);
        }
    }

    #[test]
    fn platform_restrictions_preserve_the_profile_contract() {
        let selected = LocalExecutionSelection::from_services(
            &local(LocalDevice::Metal, 0),
            &AiService::Disabled,
            false,
        );
        assert_eq!(selected.is_ok(), cfg!(target_os = "macos"));
        assert!(
            LocalExecutionSelection::from_services(
                &local(LocalDevice::Metal, 1),
                &AiService::Disabled,
                false,
            )
            .is_err()
        );
    }

    fn slot(name: &str, task: TaskKind) -> SlotSettings {
        SlotSettings {
            name: name.to_owned(),
            selection: SelectionSettings {
                id: if task == TaskKind::Rerank {
                    "bge-reranker-v2-m3"
                } else {
                    "embeddinggemma2"
                }
                .to_owned(),
                revision: "fixture-revision".to_owned(),
                task,
            },
            bundle: PathBuf::from("not-captured-by-selection"),
            manifest_relative: PathBuf::from("manifest.json"),
            runtime: if task == TaskKind::Rerank {
                RuntimeSettings::BgeRerankerV2M3 {
                    threads: NonZeroU32::new(1).unwrap(),
                    queue_capacity: NonZeroUsize::new(1).unwrap(),
                }
            } else {
                RuntimeSettings::EmbeddingGemma2 {
                    threads: NonZeroU32::new(1).unwrap(),
                    queue_capacity: NonZeroUsize::new(1).unwrap(),
                }
            },
        }
    }

    #[test]
    fn only_active_local_purposes_are_selected_for_artifact_capture() {
        let settings = ModelSettings {
            slots: vec![
                slot("embedding", TaskKind::Embedding),
                slot("rerank", TaskKind::Rerank),
                slot("alternative_embedding", TaskKind::Embedding),
            ],
        };
        let embedding_only = LocalExecutionSelection::from_services(
            &local(LocalDevice::Cpu, 0),
            &local(LocalDevice::Cpu, 0),
            false,
        )
        .unwrap();
        assert_eq!(
            settings
                .local_slots(embedding_only)
                .map(|slot| slot.name.as_str())
                .collect::<Vec<_>>(),
            ["embedding", "alternative_embedding"]
        );
        let rerank_only = LocalExecutionSelection::from_services(
            &AiService::Broker,
            &local(LocalDevice::Cpu, 0),
            true,
        )
        .unwrap();
        assert_eq!(
            settings
                .local_slots(rerank_only)
                .map(|slot| slot.name.as_str())
                .collect::<Vec<_>>(),
            ["rerank"]
        );
        let remote_only =
            LocalExecutionSelection::from_services(&AiService::Broker, &AiService::Broker, false)
                .unwrap();
        assert_eq!(settings.local_slots(remote_only).count(), 0);
    }
}
