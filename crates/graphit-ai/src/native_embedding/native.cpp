#include "graphit-ai/src/native_embedding/bridge.rs.h"
#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <limits>
#include <mutex>
#include <stdexcept>
#include <string>
#include <vector>
namespace graphit::embedding {
namespace {
void diagnostic_log(ggml_log_level level, const char* message, void*) {
    // Keep upstream failure details, including CUDA errors before GGML_ABORT.
    // Verbose model metadata remains behind the explicit SDK debug setting.
    if ((level == GGML_LOG_LEVEL_ERROR || level == GGML_LOG_LEVEL_WARN) && message) {
        std::fputs(message, stderr);
        std::fflush(stderr);
    }
}
struct Publication {
    std::mutex mutex;
    std::weak_ptr<Kernel> kernel;
    bool booted = false;
    bool accepting = false;
};
Publication& publication() { static Publication state; return state; }
std::shared_ptr<Kernel> active_kernel() {
    auto& state = publication();
    std::lock_guard<std::mutex> lock(state.mutex);
    if (!state.accepting) throw std::runtime_error("native_sdk_root_closed");
    auto kernel = state.kernel.lock();
    if (!kernel) throw std::runtime_error("native_sdk_kernel_unavailable");
    return kernel;
}
}
class Kernel {
public:
    Kernel() {
        const char* scheduler_debug = std::getenv("GGML_SCHED_DEBUG");
        const bool debug = scheduler_debug && std::atoi(scheduler_debug) > 0;
        llama_log_set(debug ? nullptr : diagnostic_log, nullptr);
        ggml_log_set(debug ? nullptr : diagnostic_log, nullptr);
        llama_backend_init();
    }
    ~Kernel() { llama_backend_free(); }
    ggml_backend_dev_t select_device(uint8_t requested, unsigned device_id, uint8_t& actual) {
        // CPU and accelerators come from the same Cargo-built engine.
        unsigned cuda_index = 0, metal_index = 0;
        for (size_t i = 0; i < ggml_backend_dev_count(); ++i) {
            auto device = ggml_backend_dev_get(i);
            if (ggml_backend_dev_type(device) != GGML_BACKEND_DEVICE_TYPE_GPU) continue;
            const std::string backend = ggml_backend_reg_name(ggml_backend_dev_backend_reg(device));
            uint8_t provider;
            unsigned index;
            if (backend == "CUDA") { provider = 1; index = cuda_index++; }
            else if (backend == "Metal") { provider = 2; index = metal_index++; }
            else continue;
            if ((requested == 3 || requested == provider) && index == device_id) {
                actual = provider;
                return device;
            }
        }
        return nullptr;
    }
};
NativeSdkRoot::NativeSdkRoot() {
    auto& state = publication();
    std::lock_guard<std::mutex> lock(state.mutex);
    if (state.booted) throw std::runtime_error("native_sdk_root_already_initialized");
    kernel_ = std::make_shared<Kernel>();
    state.kernel = kernel_; state.accepting = true; state.booted = true;
}
NativeSdkRoot::~NativeSdkRoot() {
    auto& state = publication();
    { std::lock_guard<std::mutex> lock(state.mutex); state.accepting = false; }
    kernel_.reset();
}
std::unique_ptr<NativeSdkRoot> make_sdk_root() { return std::make_unique<NativeSdkRoot>(); }
NativeOwner::Borrow::Borrow(Slot& s, const RequestState& request) : slot(s) {
    if (slot.request) throw std::runtime_error("active_request_overlap");
    slot.request = &request;
}
NativeOwner::Borrow::~Borrow() { slot.request = nullptr; }
bool NativeOwner::progress(float, void* data) {
    const auto& slot = *static_cast<Slot*>(data);
    return slot.request && request_check(*slot.request, 1);
}
bool NativeOwner::abort(void* data) {
    const auto& slot = *static_cast<Slot*>(data);
    return !slot.request || !request_check(*slot.request, 2);
}
NativeOwner::NativeOwner(rust::Str path, const RequestState& request, unsigned threads, uint8_t precision, uint8_t provider, unsigned device_id, uint8_t task)
    : kernel_(active_kernel()), threads_(threads), task_(task) {
    if (!threads || threads > static_cast<unsigned>(std::numeric_limits<int>::max()))
        throw std::runtime_error("native_thread_limit");
    Borrow borrow(slot_, request);
    if (!request_check(request, 0)) throw std::runtime_error("original_context_before_load");
    if (provider > 3 || (provider == 0 && device_id != 0))
        throw std::runtime_error("native_device_configuration");
    if (provider != 0) {
        devices_[0] = kernel_->select_device(provider, device_id, provider_);
        if (!devices_[0] && provider != 3)
            throw std::runtime_error("native_device_unavailable");
        if (devices_[0]) device_id_ = device_id;
    }
    if (!request_check(request, 0)) throw std::runtime_error("original_context_before_load");
    auto params = llama_model_default_params();
    params.devices = devices_.data();
    params.n_gpu_layers = provider_ == 0 ? 0 : -1;
    params.split_mode = LLAMA_SPLIT_MODE_NONE; params.main_gpu = 0;
    params.check_tensors = true;
    params.progress_callback = progress; params.progress_callback_user_data = &slot_;
    model_.reset(llama_model_load_from_file(std::string(path).c_str(), params));
    if (!model_) throw std::runtime_error("native_model_load_null");
    char architecture[128];
    const int length = llama_model_meta_val_str(model_.get(), "general.architecture", architecture, sizeof(architecture));
    const std::string expected_architecture = task_ == 0 ? "gemma-embedding2" : "bert";
    if (task_ > 1 || length <= 0 || length >= static_cast<int>(sizeof(architecture)) || std::string(architecture) != expected_architecture)
        throw std::runtime_error("native_architecture_mismatch");
    if (task_ == 0) {
        if (llama_model_n_embd_out(model_.get()) != 768) throw std::runtime_error("native_output_width");
        const llama_ftype expected = precision == 0 ? LLAMA_FTYPE_MOSTLY_BF16 : LLAMA_FTYPE_ALL_F32;
        if (precision > 1 || llama_model_ftype(model_.get()) != expected) throw std::runtime_error("native_precision_mismatch");
    } else {
        if (precision != 2 || llama_model_ftype(model_.get()) != LLAMA_FTYPE_MOSTLY_F16)
            throw std::runtime_error("native_precision_mismatch");
        if (llama_model_n_embd(model_.get()) != 1024 || llama_model_n_layer(model_.get()) != 24 ||
            llama_model_n_cls_out(model_.get()) != 1 || llama_model_n_ctx_train(model_.get()) != 8192)
            throw std::runtime_error("native_rerank_architecture_mismatch");
        const auto* vocab = llama_model_get_vocab(model_.get());
        if (llama_vocab_bos(vocab) != 0 || llama_vocab_eos(vocab) != 2 || llama_vocab_sep(vocab) != 2)
            throw std::runtime_error("native_rerank_special_tokens_mismatch");
    }
    if (!request_check(request, 3)) throw std::runtime_error("original_context_after_load");
}
NativeOwner::~NativeOwner() { model_.reset(); }
NativeDeviceInfo NativeOwner::registered_device() const {
    return NativeDeviceInfo{provider_, device_id_};
}
const float* NativeOwner::encode(const std::vector<llama_token>& tokens, const RequestState& request, NativeResult& result, Context& context, llama_flash_attn_type flash_attention) {
    if (!request_check(request, 0)) { result.kind = 1; return nullptr; }
    if (!context) {
        auto params = llama_context_default_params();
        // The first document window is the largest; retain only this operation's context.
        params.n_ctx = static_cast<unsigned>(tokens.size()); params.n_batch = static_cast<unsigned>(tokens.size()); params.n_ubatch = static_cast<unsigned>(tokens.size()); params.n_seq_max = 1;
        params.n_threads = static_cast<int>(threads_); params.n_threads_batch = static_cast<int>(threads_);
        params.embeddings = true; params.pooling_type = LLAMA_POOLING_TYPE_MEAN;
        params.attention_type = LLAMA_ATTENTION_TYPE_NON_CAUSAL;
        params.flash_attn_type = provider_ == 0 ? flash_attention : LLAMA_FLASH_ATTN_TYPE_ENABLED;
        params.type_k = GGML_TYPE_F32; params.type_v = GGML_TYPE_F32;
        params.offload_kqv = provider_ != 0; params.op_offload = provider_ != 0;
        params.abort_callback = abort; params.abort_callback_data = &slot_;
        context.reset(llama_init_from_model(model_.get(), params));
        if (!context) { result.kind = 5; return nullptr; }
    }
    llama_memory_clear(llama_get_memory(context.get()), true);
    std::unique_ptr<llama_batch_ext, decltype(&llama_batch_ext_free)> batch(llama_batch_ext_init(context.get()), llama_batch_ext_free);
    if (!batch) { result.kind = 6; return nullptr; }
    for (size_t i = 0; i < tokens.size(); ++i) {
        const int index = llama_batch_ext_add_token(batch.get(), 0, tokens[i]);
        const llama_pos position = static_cast<llama_pos>(i);
        if (index < 0 || !llama_batch_ext_set_pos(batch.get(), index, &position) || !llama_batch_ext_set_output_embd(batch.get(), index, true)) {
            result.kind = 7; return nullptr;
        }
    }
    result.processed = true;
    result.native_status = llama_process(context.get(), LLAMA_PROCESS_TYPE_ENCODE, batch.get());
    if (result.native_status != 0) { result.kind = 8; return nullptr; }
    if (!request_check(request, 0)) { result.kind = 1; return nullptr; }
    const float* output = llama_get_embeddings_seq(context.get(), 0);
    if (!output) { result.kind = 9; return nullptr; }
    return output;
}
NativeResult NativeOwner::embed(rust::Str text, const RequestState& request, unsigned max_tokens, unsigned document_prefix_bytes) {
    if (task_ != 0) throw std::runtime_error("native_task_mismatch");
    Borrow borrow(slot_, request);
    // Release the compute graph on every return, while the original callback slot
    // is still borrowed. Only the verified model remains resident between calls.
    Context context{nullptr, llama_free};
    NativeResult result{};
    if (!request_check(request, 0)) { result.kind = 1; return result; }
    if (text.empty() || text.size() > static_cast<size_t>(std::numeric_limits<int>::max()) || !max_tokens || max_tokens > 8192 || document_prefix_bytes >= text.size()) {
        result.kind = 2; return result;
    }
    const auto* vocab = llama_model_get_vocab(model_.get());
    int count = llama_tokenize(vocab, text.data(), static_cast<int>(text.size()), nullptr, 0, true, true);
    if (count >= 0 || count == std::numeric_limits<int>::min()) { result.kind = 3; return result; }
    result.token_count = static_cast<unsigned>(-count);
    std::array<float, 768> pooled{};
    const float* output = nullptr;
    if (result.token_count <= max_tokens) {
        std::vector<llama_token> tokens(result.token_count);
        count = llama_tokenize(vocab, text.data(), static_cast<int>(text.size()), tokens.data(), static_cast<int>(tokens.size()), true, true);
        if (count <= 0 || static_cast<size_t>(count) > tokens.size()) { result.kind = 3; return result; }
        tokens.resize(static_cast<size_t>(count)); result.token_count = static_cast<unsigned>(count);
        output = encode(tokens, request, result, context, LLAMA_FLASH_ATTN_TYPE_DISABLED);
        if (!output) return result;
    } else {
        if (!document_prefix_bytes) { result.kind = 4; return result; }
        const auto tokenize = [&](const char* data, size_t bytes, bool special, std::vector<llama_token>& tokens) {
            int size = llama_tokenize(vocab, data, static_cast<int>(bytes), nullptr, 0, special, true);
            if (size >= 0 || size == std::numeric_limits<int>::min()) return false;
            tokens.resize(static_cast<size_t>(-size));
            size = llama_tokenize(vocab, data, static_cast<int>(bytes), tokens.data(), static_cast<int>(tokens.size()), special, true);
            if (size <= 0 || static_cast<size_t>(size) > tokens.size()) return false;
            tokens.resize(static_cast<size_t>(size));
            return true;
        };
        // Keep native token IDs intact, including Unicode byte-fallback and parsed specials.
        // The SDK inserts the configured BOS into the prefix; EOS belongs after each body window.
        std::vector<llama_token> prefix, body;
        if (!tokenize(text.data(), document_prefix_bytes, true, prefix) ||
            !tokenize(text.data() + document_prefix_bytes, text.size() - document_prefix_bytes, false, body)) {
            result.kind = 3; return result;
        }
        const bool eos = llama_vocab_get_add_eos(vocab);
        if (eos) {
            if (prefix.empty() || prefix.back() != llama_vocab_eos(vocab)) { result.kind = 3; return result; }
            prefix.pop_back();
        }
        const size_t overhead = prefix.size() + static_cast<size_t>(eos);
        if (overhead >= max_tokens) { result.kind = 4; return result; }
        const size_t capacity = max_tokens - overhead;
        std::array<double, 768> weighted{};
        std::vector<llama_token> window;
        window.reserve(max_tokens);
        for (size_t offset = 0; offset < body.size();) {
            if (!request_check(request, 0)) { result.kind = 1; return result; }
            const size_t consumed = std::min(capacity, body.size() - offset);
            window.assign(prefix.begin(), prefix.end());
            window.insert(window.end(), body.begin() + offset, body.begin() + offset + consumed);
            if (eos) window.push_back(llama_vocab_eos(vocab));
            output = encode(window, request, result, context, LLAMA_FLASH_ATTN_TYPE_AUTO);
            if (!output) return result;
            // Copy the raw sequence MEAN before the next clear/reset; repeated prefix/specials
            // do not contribute to the weight. No intermediate L2 or partial result escapes.
            for (size_t i = 0; i < weighted.size(); ++i) {
                if (!std::isfinite(output[i])) { result.kind = 10; return result; }
                weighted[i] += static_cast<double>(output[i]) * static_cast<double>(consumed);
            }
            offset += consumed;
            if (!request_check(request, 5)) { result.kind = 1; return result; }
        }
        for (size_t i = 0; i < pooled.size(); ++i)
            pooled[i] = static_cast<float>(weighted[i] / static_cast<double>(body.size()));
        output = pooled.data();
    }
    float sum = 0;
    for (size_t i = 0; i < 768; ++i) {
        if (!std::isfinite(output[i])) { result.kind = 10; return result; }
        sum += output[i] * output[i];
    }
    const float norm = std::sqrt(sum);
    if (!std::isfinite(norm) || norm <= 0) { result.kind = 10; return result; }
    for (size_t i = 0; i < 768; ++i) result.values.push_back(output[i] / norm);
    if (!request_check(request, 3)) { result.kind = 1; result.values.clear(); }
    return result;
}
NativePair NativeOwner::tokenize_pair(rust::Str query, rust::Str document, const RequestState& request, unsigned max_tokens) {
    if (task_ != 1) throw std::runtime_error("native_task_mismatch");
    Borrow borrow(slot_, request);
    NativePair result{};
    if (!request_check(request, 0)) { result.kind = 1; return result; }
    if (max_tokens < 4 || max_tokens > 8192 || query.size() > static_cast<size_t>(std::numeric_limits<int>::max()) ||
        document.size() > static_cast<size_t>(std::numeric_limits<int>::max())) { result.kind = 2; return result; }
    const auto* vocab = llama_model_get_vocab(model_.get());
    const auto tokenize = [&](rust::Str text, std::vector<llama_token>& tokens) {
        int count = llama_tokenize(vocab, text.data(), static_cast<int>(text.size()), nullptr, 0, false, false);
        if (count == 0) return true;
        if (count > 0 || count == std::numeric_limits<int>::min()) return false;
        tokens.resize(static_cast<size_t>(-count));
        count = llama_tokenize(vocab, text.data(), static_cast<int>(text.size()), tokens.data(), static_cast<int>(tokens.size()), false, false);
        if (count < 0 || static_cast<size_t>(count) > tokens.size()) return false;
        tokens.resize(static_cast<size_t>(count));
        return true;
    };
    std::vector<llama_token> q, d;
    if (!tokenize(query, q) || !tokenize(document, d)) { result.kind = 3; return result; }
    if (!request_check(request, 0)) { result.kind = 1; return result; }
    const size_t original = q.size() + d.size() + 4;
    if (original > std::numeric_limits<unsigned>::max()) { result.kind = 2; return result; }
    result.original_token_count = static_cast<unsigned>(original);
    // Match tokenizers 0.23.2 LongestFirst/right: preserve the shorter sequence,
    // then split the remaining budget, assigning the odd token to the longer one.
    const size_t capacity = max_tokens - 4;
    if (q.size() + d.size() > capacity) {
        size_t n1 = q.size(), n2 = d.size();
        const bool swap = n1 > n2;
        if (swap) std::swap(n1, n2);
        n2 = n1 > capacity ? n1 : std::max(n1, capacity - n1);
        if (n1 + n2 > capacity) { n1 = capacity / 2; n2 = n1 + capacity % 2; }
        if (swap) std::swap(n1, n2);
        q.resize(n1); d.resize(n2);
    }
    std::vector<llama_token> tokens;
    tokens.reserve(q.size() + d.size() + 4);
    tokens.push_back(llama_vocab_bos(vocab));
    tokens.insert(tokens.end(), q.begin(), q.end());
    tokens.push_back(llama_vocab_eos(vocab)); tokens.push_back(llama_vocab_sep(vocab));
    tokens.insert(tokens.end(), d.begin(), d.end()); tokens.push_back(llama_vocab_eos(vocab));
    for (const auto token : tokens) result.ids.push_back(token);
    if (!request_check(request, 0)) { result.kind = 1; result.ids.clear(); }
    return result;
}
NativeRerankResult NativeOwner::rerank_tokens(rust::Slice<const int32_t> tokens, const RequestState& request) {
    if (task_ != 1) throw std::runtime_error("native_task_mismatch");
    Borrow borrow(slot_, request);
    Context context{nullptr, llama_free};
    NativeRerankResult result{};
    if (!request_check(request, 0)) { result.kind = 1; return result; }
    if (tokens.size() < 4 || tokens.size() > 8192) { result.kind = 2; return result; }
    const auto* vocab = llama_model_get_vocab(model_.get());
    for (const auto token : tokens) {
        if (token < 0 || token >= llama_vocab_n_tokens(vocab)) { result.kind = 2; return result; }
    }
    result.token_count = static_cast<unsigned>(tokens.size());
    auto params = llama_context_default_params();
    // Bound the operation by its actual complete pair. No fixed 8192-token workspace survives it.
    params.n_ctx = result.token_count; params.n_batch = result.token_count; params.n_ubatch = result.token_count; params.n_seq_max = 1;
    params.n_threads = static_cast<int>(threads_); params.n_threads_batch = static_cast<int>(threads_);
    params.embeddings = true; params.pooling_type = LLAMA_POOLING_TYPE_RANK;
    params.attention_type = LLAMA_ATTENTION_TYPE_NON_CAUSAL;
    params.flash_attn_type = provider_ == 0 ? LLAMA_FLASH_ATTN_TYPE_AUTO : LLAMA_FLASH_ATTN_TYPE_ENABLED;
    params.type_k = GGML_TYPE_F32; params.type_v = GGML_TYPE_F32;
    params.offload_kqv = provider_ != 0; params.op_offload = provider_ != 0;
    params.abort_callback = abort; params.abort_callback_data = &slot_;
    context.reset(llama_init_from_model(model_.get(), params));
    if (!context) { result.kind = 5; return result; }
    result.context_tokens = llama_n_ctx(context.get());
    std::unique_ptr<llama_batch_ext, decltype(&llama_batch_ext_free)> batch(llama_batch_ext_init(context.get()), llama_batch_ext_free);
    if (!batch) { result.kind = 6; return result; }
    for (size_t i = 0; i < tokens.size(); ++i) {
        const int index = llama_batch_ext_add_token(batch.get(), 0, tokens[i]);
        const llama_pos position = static_cast<llama_pos>(i);
        if (index < 0 || !llama_batch_ext_set_pos(batch.get(), index, &position) || !llama_batch_ext_set_output_embd(batch.get(), index, true)) {
            result.kind = 7; return result;
        }
    }
    if (!request_check(request, 0)) { result.kind = 1; return result; }
    result.processed = true;
    result.native_status = llama_process(context.get(), LLAMA_PROCESS_TYPE_ENCODE, batch.get());
    if (result.native_status != 0) { result.kind = 8; return result; }
    if (!request_check(request, 0)) { result.kind = 1; return result; }
    const float* scores = llama_get_embeddings_seq(context.get(), 0);
    if (!scores || !std::isfinite(scores[0])) { result.kind = 9; return result; }
    result.score = scores[0];
    if (!request_check(request, 3)) { result.kind = 1; return result; }
    return result;
}
std::unique_ptr<NativeOwner> make_owner(rust::Str path, const RequestState& request, unsigned threads, uint8_t precision, uint8_t provider, unsigned device_id, uint8_t task) {
    return std::make_unique<NativeOwner>(path, request, threads, precision, provider, device_id, task);
}
}
