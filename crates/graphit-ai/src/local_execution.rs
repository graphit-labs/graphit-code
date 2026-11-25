//! Data-only requested and effective device selection for local GGUF execution.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LocalProvider {
    Auto,
    CPU,
    CUDA,
    Metal,
}
/// Registration reports native session setup, not the placement of every graph node.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LocalDeviceFallback {
    AcceleratorUnavailable,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LocalDeviceRegistration {
    pub requested_provider: LocalProvider,
    pub requested_device_id: i32,
    pub provider: LocalProvider,
    pub device_id: i32,
    pub fallback_reason: Option<LocalDeviceFallback>,
}
impl LocalDeviceRegistration {
    /// Validate reported selection, without claiming GPU placement of every operation.
    pub fn matches_request(self, provider: LocalProvider, device_id: i32) -> bool {
        if self.requested_provider != provider || self.requested_device_id != device_id {
            return false;
        }
        match provider {
            LocalProvider::Auto => match self.provider {
                LocalProvider::CPU => {
                    self.device_id == 0
                        && self.fallback_reason == Some(LocalDeviceFallback::AcceleratorUnavailable)
                }
                LocalProvider::CUDA | LocalProvider::Metal => {
                    self.device_id == device_id && self.fallback_reason.is_none()
                }
                LocalProvider::Auto => false,
            },
            explicit => {
                self.provider == explicit
                    && self.device_id == device_id
                    && self.fallback_reason.is_none()
            }
        }
    }
}

#[cfg(test)]
mod device_metadata_tests {
    use super::*;

    fn cpu(requested: LocalProvider) -> LocalDeviceRegistration {
        LocalDeviceRegistration {
            requested_provider: requested,
            requested_device_id: 0,
            provider: LocalProvider::CPU,
            device_id: 0,
            fallback_reason: None,
        }
    }

    #[test]
    fn explicit_gpu_cannot_report_cpu_success() {
        let mut result = cpu(LocalProvider::CUDA);
        assert!(!result.matches_request(LocalProvider::CUDA, 0));
        result.fallback_reason = Some(LocalDeviceFallback::AcceleratorUnavailable);
        assert!(!result.matches_request(LocalProvider::CUDA, 0));
        assert!(cpu(LocalProvider::CPU).matches_request(LocalProvider::CPU, 0));
    }

    #[test]
    fn auto_cpu_requires_fallback_reason_and_original_index() {
        let mut result = cpu(LocalProvider::Auto);
        assert!(!result.matches_request(LocalProvider::Auto, 0));
        result.fallback_reason = Some(LocalDeviceFallback::AcceleratorUnavailable);
        result.requested_device_id = 2;
        assert!(result.matches_request(LocalProvider::Auto, 2));
        assert!(!result.matches_request(LocalProvider::Auto, 0));
        result.device_id = 2;
        assert!(!result.matches_request(LocalProvider::Auto, 2));
    }

    #[test]
    fn accelerator_registration_keeps_actual_index_and_has_no_fallback() {
        let mut result = LocalDeviceRegistration {
            requested_provider: LocalProvider::Auto,
            requested_device_id: 2,
            provider: LocalProvider::CUDA,
            device_id: 2,
            fallback_reason: None,
        };
        assert!(result.matches_request(LocalProvider::Auto, 2));
        result.device_id = 1;
        assert!(!result.matches_request(LocalProvider::Auto, 2));
        result.provider = LocalProvider::Auto;
        assert!(!result.matches_request(LocalProvider::Auto, 2));
    }
}
