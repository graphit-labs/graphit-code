use super::{RequestState, request_check};

// CXX checks the ABI. The native owner retains a request only during a synchronous
// call, clears its stable callback slot with RAII, and returns owned values.
#[allow(unsafe_code)]
#[cxx::bridge(namespace = "graphit::embedding")]
pub(crate) mod ffi {
    struct NativeDeviceInfo {
        provider: u8,
        device_id: u32,
    }
    struct NativePair {
        kind: u8,
        original_token_count: u32,
        ids: Vec<i32>,
    }
    struct NativeRerankResult {
        kind: u8,
        native_status: i32,
        processed: bool,
        token_count: u32,
        context_tokens: u32,
        score: f32,
    }
    struct NativeResult {
        kind: u8,
        native_status: i32,
        processed: bool,
        token_count: u32,
        values: Vec<f32>,
    }
    extern "Rust" {
        type RequestState;
        fn request_check(request: &RequestState, phase: u8) -> bool;
    }
    unsafe extern "C++" {
        include!("src/native_embedding/native.h");
        type NativeSdkRoot;
        fn make_sdk_root() -> Result<UniquePtr<NativeSdkRoot>>;
        type NativeOwner;
        fn make_owner(
            path: &str,
            request: &RequestState,
            threads: u32,
            precision: u8,
            provider: u8,
            device_id: u32,
            task: u8,
        ) -> Result<UniquePtr<NativeOwner>>;
        fn tokenize_pair(
            self: Pin<&mut NativeOwner>,
            query: &str,
            document: &str,
            request: &RequestState,
            max_tokens: u32,
        ) -> Result<NativePair>;
        fn rerank_tokens(
            self: Pin<&mut NativeOwner>,
            tokens: &[i32],
            request: &RequestState,
        ) -> Result<NativeRerankResult>;
        fn registered_device(self: &NativeOwner) -> NativeDeviceInfo;
        fn embed(
            self: Pin<&mut NativeOwner>,
            text: &str,
            request: &RequestState,
            max_tokens: u32,
            document_prefix_bytes: u32,
        ) -> Result<NativeResult>;
    }
}
