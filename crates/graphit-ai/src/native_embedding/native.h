#pragma once
#include "rust/cxx.h"
#include "llama.h"
#include "ggml-backend.h"
#include <array>
#include <memory>
#include <vector>
namespace graphit::embedding {
struct RequestState;
struct NativeResult;
struct NativeRerankResult;
struct NativePair;
struct NativeDeviceInfo;
class Kernel;
class NativeSdkRoot {
public:
    NativeSdkRoot();
    ~NativeSdkRoot();
private:
    std::shared_ptr<Kernel> kernel_;
};
std::unique_ptr<NativeSdkRoot> make_sdk_root();
class NativeOwner {
public:
    NativeOwner(rust::Str path, const RequestState& request, unsigned threads, uint8_t precision, uint8_t provider, unsigned device_id, uint8_t task);
    ~NativeOwner();
    NativeDeviceInfo registered_device() const;
    NativePair tokenize_pair(rust::Str query, rust::Str document, const RequestState& request, unsigned max_tokens);
    NativeRerankResult rerank_tokens(rust::Slice<const int32_t> tokens, const RequestState& request);
    NativeResult embed(rust::Str text, const RequestState& request, unsigned max_tokens, unsigned document_prefix_bytes);
private:
    std::shared_ptr<Kernel> kernel_;
    unsigned threads_;
    uint8_t task_;
    uint8_t provider_ = 0;
    unsigned device_id_ = 0;
    std::array<ggml_backend_dev_t, 2> devices_{};
    struct Slot { const RequestState* request = nullptr; } slot_;
    std::unique_ptr<llama_model, decltype(&llama_model_free)> model_{nullptr, llama_model_free};
    using Context = std::unique_ptr<llama_context, decltype(&llama_free)>;
    const float* encode(const std::vector<llama_token>& tokens, const RequestState& request, NativeResult& result, Context& context, llama_flash_attn_type flash_attention);
    static bool progress(float, void*);
    static bool abort(void*);
    struct Borrow {
        Slot& slot;
        Borrow(Slot& slot, const RequestState& request);
        ~Borrow();
    };
};
std::unique_ptr<NativeOwner> make_owner(rust::Str path, const RequestState& request, unsigned threads, uint8_t precision, uint8_t provider, unsigned device_id, uint8_t task);
}
