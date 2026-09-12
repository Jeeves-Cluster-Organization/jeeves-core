#include <jeeves/llm/llamacpp.hpp>

#include "tool_calls.hpp"
#include "native_chat.hpp"

#include <llama-cpp.h>

#include <algorithm>
#include <climits>
#include <cmath>
#include <limits>
#include <mutex>
#include <thread>
#include <unordered_map>

namespace jeeves {
namespace {

using SharedModel = std::shared_ptr<llama_model>;

void init_backend() {
    static std::once_flag once;
    std::call_once(once, [] { llama_backend_init(); });
}

template <class T>
T setting(const ModelRequest & request, const char * name, T fallback) {
    if (!request.extra_body || !request.extra_body->contains(name)) return fallback;
    return request.extra_body->at(name).get<T>();
}

Result<void> validate_request(const ModelRequest & request) {
    if (request.extra_body && !request.extra_body->is_object())
        return std::unexpected(Error::invalid_input("LLM extra_body must be an object"));
    if (request.temperature && (!std::isfinite(*request.temperature) || std::abs(*request.temperature) > std::numeric_limits<float>::max()))
        return std::unexpected(Error::invalid_input("LLM temperature must be finite and fit a float"));
    if (request.max_tokens && (*request.max_tokens == 0 || *request.max_tokens > INT32_MAX))
        return std::unexpected(Error::invalid_input("llama.cpp max_tokens must be between 1 and INT32_MAX"));
    if (!request.extra_body) return {};
    for (const auto * key : {"n_predict", "n_ctx", "n_threads", "n_gpu_layers", "top_k", "seed"}) {
        if (!request.extra_body->contains(key)) continue;
        const auto & value = request.extra_body->at(key);
        const double minimum = std::string(key) == "n_gpu_layers" ? -1 :
            (std::string(key) == "n_predict" || std::string(key) == "n_threads" ? 1 : 0);
        const double maximum = std::string(key) == "seed" ? UINT32_MAX : INT32_MAX;
        if (!value.is_number_integer() || value.get<double>() < minimum || value.get<double>() > maximum)
            return std::unexpected(Error::invalid_input(std::string("invalid llama.cpp setting: ") + key));
    }
    for (const auto * key : {"top_p", "min_p"}) {
        if (!request.extra_body->contains(key)) continue;
        const auto & value = request.extra_body->at(key);
        if (!value.is_number() || !std::isfinite(value.get<double>()) || value.get<double>() < 0 || value.get<double>() > 1)
            return std::unexpected(Error::invalid_input(std::string("invalid llama.cpp setting: ") + key));
    }
    if (request.extra_body->contains("grammar") && !request.extra_body->at("grammar").is_string())
        return std::unexpected(Error::invalid_input("llama.cpp grammar must be a string"));
    return {};
}

std::string role_name(Role role) {
    switch (role) {
        case Role::System: return "system";
        case Role::User: return "user";
        case Role::Assistant: return "assistant";
        case Role::Tool: return "tool";
    }
    return "user";
}

std::string message_content(const Message & message) {
    std::string content = message.content;
    if (message.role == Role::Tool && message.tool_call_id)
        content = "Tool call " + *message.tool_call_id + ": " + content;
    for (const auto & call : message.tool_calls) {
        json encoded{{"id", call.id}, {"name", call.name}, {"arguments", call.arguments}};
        content += "<tool_call>" + encoded.dump() + "</tool_call>";
    }
    return content;
}

std::string tool_instructions(const std::vector<ToolSpec> & tools) {
    if (tools.empty()) return {};
    json specs = json::array();
    for (const auto & tool : tools)
        specs.push_back({{"name", tool.name}, {"description", tool.description}, {"parameters", tool.parameters}});
    return "\nAvailable tools (emit <tool_call>{\"name\":...,\"arguments\":...}</tool_call>): " + specs.dump();
}

Result<std::string> apply_chat_template(llama_model * model, const ModelRequest & request) {
    std::vector<std::string> roles, contents;
    roles.reserve(request.messages.size()); contents.reserve(request.messages.size());
    for (const auto & message : request.messages) {
        roles.push_back(role_name(message.role)); contents.push_back(message_content(message));
    }
    std::string hint = tool_instructions(request.tools);
    if (request.response_schema) hint += "\nReturn JSON matching this schema: " + request.response_schema->dump();
    if (!hint.empty()) {
        if (!contents.empty() && request.messages.front().role == Role::System) contents.front() += hint;
        else { roles.insert(roles.begin(), "system"); contents.insert(contents.begin(), std::move(hint)); }
    }
    std::vector<llama_chat_message> messages;
    messages.reserve(roles.size());
    for (std::size_t i = 0; i < roles.size(); ++i) messages.push_back({roles[i].c_str(), contents[i].c_str()});
    const char * chat_template = llama_model_chat_template(model, nullptr);
    char architecture[64]{};
    llama_model_meta_val_str(model, "general.architecture", architecture, sizeof(architecture));
    const std::string_view embedded = chat_template ? chat_template : "";
    if (const auto * chat = detail::native_chat_for(architecture, embedded))
        return chat->prompt(request);
    int32_t needed = llama_chat_apply_template(chat_template, messages.data(), messages.size(), true, nullptr, 0);
    if (needed < 0)
        return std::unexpected(Error::configuration("llama.cpp could not apply the model chat template"));
    std::string prompt(static_cast<std::size_t>(needed) + 1, '\0');
    int32_t written = llama_chat_apply_template(chat_template, messages.data(), messages.size(), true,
                                                prompt.data(), static_cast<int32_t>(prompt.size()));
    if (written < 0) return std::unexpected(Error::configuration("llama.cpp could not apply the model chat template"));
    prompt.resize(static_cast<std::size_t>(written));
    return prompt;
}

Result<std::vector<llama_token>> tokenize(const llama_vocab * vocab, const std::string & prompt) {
    if (prompt.size() > INT32_MAX)
        return std::unexpected(Error::invalid_input("llama.cpp prompt is too large"));
    int32_t size = static_cast<int32_t>(std::min<std::size_t>(prompt.size() + 32, INT32_MAX));
    std::vector<llama_token> tokens(static_cast<std::size_t>(size));
    int32_t count = llama_tokenize(vocab, prompt.data(), static_cast<int32_t>(prompt.size()),
                                   tokens.data(), size, true, true);
    if (count < 0 && count != INT32_MIN) {
        tokens.resize(static_cast<std::size_t>(-count));
        count = llama_tokenize(vocab, prompt.data(), static_cast<int32_t>(prompt.size()),
                               tokens.data(), static_cast<int32_t>(tokens.size()), true, true);
    }
    if (count < 0) return std::unexpected(Error::invalid_input("llama.cpp could not tokenize the prompt"));
    tokens.resize(static_cast<std::size_t>(count));
    return tokens;
}

class LlamaStream final : public ModelStream {
public:
    static Result<std::unique_ptr<ModelStream>> create(SharedModel model, const ModelRequest & request) {
        auto prompt = apply_chat_template(model.get(), request);
        if (!prompt) return std::unexpected(prompt.error());
        const auto * vocab = llama_model_get_vocab(model.get());
        auto tokens = tokenize(vocab, *prompt);
        if (!tokens) return std::unexpected(tokens.error());
        auto params = llama_context_default_params();
        const auto max_tokens = request.max_tokens.value_or(setting<std::uint32_t>(request, "n_predict", 512));
        params.n_ctx = setting<std::uint32_t>(request, "n_ctx", 4096);
        const int threads = setting<int>(request, "n_threads", std::max(1u, std::thread::hardware_concurrency()));
        params.n_threads = threads; params.n_threads_batch = threads;
        auto context = llama_context_ptr(llama_init_from_model(model.get(), params));
        if (!context) return std::unexpected(Error::unavailable("llama.cpp could not create a context"));
        if (tokens->empty() || tokens->size() >= llama_n_ctx(context.get()))
            return std::unexpected(Error::invalid_input("prompt exceeds the llama.cpp context size"));
        std::stop_token stop = request.stop;
        const char * chat_template = llama_model_chat_template(model.get(), nullptr);
        char architecture[64]{};
        llama_model_meta_val_str(model.get(), "general.architecture", architecture, sizeof(architecture));
        const auto * chat = detail::native_chat_for(architecture, chat_template ? chat_template : "");
        auto stream = std::unique_ptr<LlamaStream>(new LlamaStream(std::move(model), std::move(context),
            std::move(*tokens), max_tokens, stop, !request.tools.empty(), chat));
        llama_set_abort_callback(stream->context_.get(), [](void * data) {
            return static_cast<std::stop_token *>(data)->stop_requested();
        }, &stream->stop_);
        auto chain_params = llama_sampler_chain_default_params();
        stream->sampler_.reset(llama_sampler_chain_init(chain_params));
        if (!stream->sampler_) return std::unexpected(Error::unavailable("llama.cpp could not create a sampler"));
        const int top_k = setting<int>(request, "top_k", 40);
        const float top_p = setting<float>(request, "top_p", 0.95F);
        const float min_p = setting<float>(request, "min_p", 0.05F);
        const float temperature = static_cast<float>(request.temperature.value_or(0.8));
        const std::uint32_t seed = setting<std::uint32_t>(request, "seed", LLAMA_DEFAULT_SEED);
        std::string grammar;
        if (request.extra_body && request.extra_body->contains("grammar") && (*request.extra_body)["grammar"].is_string())
            grammar = (*request.extra_body)["grammar"].get<std::string>();
        // Thinking-channel adapters emit non-JSON tokens first. Schema checks stay
        // in the workflow; an explicit extra_body grammar still wins.
        const bool default_grammar = !chat || chat->allow_default_schema_grammar;
        if (grammar.empty() && default_grammar && request.response_schema && request.tools.empty()) grammar = R"gbnf(
root ::= value
value ::= object | array | string | number | "true" ws | "false" ws | "null" ws
object ::= "{" ws (string ":" ws value ("," ws string ":" ws value)*)? "}" ws
array ::= "[" ws (value ("," ws value)*)? "]" ws
string ::= "\"" char* "\"" ws
char ::= [^"\\\x00-\x1f] | "\\" (["\\/bfnrt] | "u" [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F] [0-9a-fA-F])
number ::= "-"? ("0" | [1-9] [0-9]*) ("." [0-9]+)? ([eE] [+-]? [0-9]+)? ws
ws ::= [ \t\n\r]*
)gbnf";
        if (!grammar.empty()) {
            auto * grammar_sampler = llama_sampler_init_grammar(vocab, grammar.c_str(), "root");
            if (!grammar_sampler) return std::unexpected(Error::invalid_input("llama.cpp grammar is invalid"));
            llama_sampler_chain_add(stream->sampler_.get(), grammar_sampler);
        }
        llama_sampler_chain_add(stream->sampler_.get(), llama_sampler_init_top_k(top_k));
        llama_sampler_chain_add(stream->sampler_.get(), llama_sampler_init_top_p(top_p, 1));
        llama_sampler_chain_add(stream->sampler_.get(), llama_sampler_init_min_p(min_p, 1));
        llama_sampler_chain_add(stream->sampler_.get(), llama_sampler_init_temp(temperature));
        llama_sampler_chain_add(stream->sampler_.get(), llama_sampler_init_dist(seed));
        const auto batch_size = llama_n_batch(stream->context_.get());
        for (std::size_t offset = 0; offset < stream->prompt_tokens_.size(); offset += batch_size) {
            if (stop.stop_requested()) return std::unexpected(Error::cancelled("llama.cpp generation cancelled"));
            const auto count = std::min<std::size_t>(batch_size, stream->prompt_tokens_.size() - offset);
            if (llama_decode(stream->context_.get(), llama_batch_get_one(stream->prompt_tokens_.data() + offset,
                             static_cast<int32_t>(count))) != 0) {
                if (stop.stop_requested()) return std::unexpected(Error::cancelled("llama.cpp generation cancelled"));
                return std::unexpected(Error::unavailable("llama.cpp failed to evaluate the prompt"));
            }
        }
        return std::unique_ptr<ModelStream>(std::move(stream));
    }

    std::optional<Result<ModelStreamEvent>> next() override {
        if (!pending_.empty()) {
            auto value = std::move(pending_.front()); pending_.pop_front(); return value;
        }
        if (done_) return std::nullopt;
        if (stop_.stop_requested()) { done_ = true; return std::unexpected(Error::cancelled("llama.cpp generation cancelled")); }
        const bool defer = chat_ && chat_->defer_visible_text;
        while (generated_ < max_tokens_ && prompt_tokens_.size() + generated_ < llama_n_ctx(context_.get())) {
            if (stop_.stop_requested()) { done_ = true; return std::unexpected(Error::cancelled("llama.cpp generation cancelled")); }
            auto piece = sample_piece();
            if (!piece) return std::unexpected(piece.error());
            if (!*piece)
                return finish(ModelStopReason::completed("stop"));
            if (!defer)
                return ModelStreamEvent(std::move(**piece));
        }
        return finish(ModelStopReason::max_tokens("length"));
    }

private:
    LlamaStream(SharedModel model, llama_context_ptr context, std::vector<llama_token> prompt,
                std::uint32_t max_tokens, std::stop_token stop, bool parse_tools,
                const detail::NativeChat * chat)
        : model_(std::move(model)), context_(std::move(context)), prompt_tokens_(std::move(prompt)),
          max_tokens_(max_tokens), stop_(stop), parse_tools_(parse_tools), chat_(chat) {}

    Result<std::optional<std::string>> sample_piece() {
        const auto token = llama_sampler_sample(sampler_.get(), context_.get(), -1);
        if (llama_vocab_is_eog(llama_model_get_vocab(model_.get()), token))
            return std::optional<std::string>{};
        std::string piece(32, '\0');
        int32_t size = llama_token_to_piece(llama_model_get_vocab(model_.get()), token, piece.data(),
                                            static_cast<int32_t>(piece.size()), 0, false);
        if (size < 0) {
            piece.resize(static_cast<std::size_t>(-size));
            size = llama_token_to_piece(llama_model_get_vocab(model_.get()), token, piece.data(),
                                        static_cast<int32_t>(piece.size()), 0, false);
        }
        if (size < 0) { done_ = true; return std::unexpected(Error::internal("llama.cpp could not decode a token")); }
        piece.resize(static_cast<std::size_t>(size));
        llama_token next_token = token;
        ++generated_;
        if (generated_ < max_tokens_ && prompt_tokens_.size() + generated_ < llama_n_ctx(context_.get()) &&
            llama_decode(context_.get(), llama_batch_get_one(&next_token, 1)) != 0) {
            done_ = true;
            if (stop_.stop_requested()) return std::unexpected(Error::cancelled("llama.cpp generation cancelled"));
            return std::unexpected(Error::unavailable("llama.cpp token evaluation failed"));
        }
        output_ += piece;
        return piece;
    }

    std::optional<Result<ModelStreamEvent>> finish(ModelStopReason reason) {
        done_ = true;
        if (chat_ && chat_->defer_visible_text)
            pending_.emplace_back(ModelStreamEvent(chat_->visible_text(output_)));
        if (parse_tools_ && reason.kind != ModelStopKind::MaxTokens) {
            auto calls = detail::parse_tool_calls(output_);
            if (!calls.empty()) reason = ModelStopReason::tool_call("tool_calls");
            for (auto & call : calls) pending_.emplace_back(ModelStreamEvent(std::move(call)));
        }
        pending_.emplace_back(ModelStreamEvent(TokenUsage{prompt_tokens_.size(), generated_}));
        pending_.emplace_back(ModelStreamEvent(std::move(reason)));
        auto value = std::move(pending_.front()); pending_.pop_front(); return value;
    }

    SharedModel model_;
    llama_context_ptr context_;
    llama_sampler_ptr sampler_;
    std::vector<llama_token> prompt_tokens_;
    std::uint32_t max_tokens_;
    std::uint64_t generated_ = 0;
    std::stop_token stop_;
    std::string output_;
    std::deque<Result<ModelStreamEvent>> pending_;
    bool done_ = false;
    bool parse_tools_ = false;
    const detail::NativeChat * chat_ = nullptr;
};

} // namespace

struct LlamaCppProvider::Impl {
    explicit Impl(std::string path) { paths.emplace("default", std::move(path)); }
    std::mutex mutex;
    std::unordered_map<std::string, std::string> paths;
    std::unordered_map<std::string, SharedModel> models;
};

LlamaCppProvider::LlamaCppProvider(std::string gguf_path) : impl_(std::make_unique<Impl>(std::move(gguf_path))) {
    init_backend();
}
LlamaCppProvider::~LlamaCppProvider() = default;
LlamaCppProvider::LlamaCppProvider(LlamaCppProvider &&) noexcept = default;
LlamaCppProvider & LlamaCppProvider::operator=(LlamaCppProvider &&) noexcept = default;
LlamaCppProvider & LlamaCppProvider::with_model_role(std::string role, std::string path) {
    std::lock_guard lock(impl_->mutex); impl_->paths[std::move(role)] = std::move(path); return *this;
}

Result<std::unique_ptr<ModelStream>> LlamaCppProvider::stream(const ModelRequest & request) {
    if (request.stop.stop_requested()) return std::unexpected(Error::cancelled("llama.cpp generation cancelled"));
    if (auto valid = validate_request(request); !valid) return std::unexpected(valid.error());
    const std::string role = request.model.value_or("default");
    SharedModel model;
    {
        std::lock_guard lock(impl_->mutex);
        const auto configured = impl_->paths.find(role);
        const auto path = configured == impl_->paths.end() ? role : configured->second;
        auto loaded = impl_->models.find(path);
        if (loaded != impl_->models.end()) model = loaded->second;
        else {
            auto params = llama_model_default_params();
            params.n_gpu_layers = setting<int>(request, "n_gpu_layers", -1);
            params.progress_callback = [](float, void * data) {
                return !static_cast<const std::stop_token *>(data)->stop_requested();
            };
            params.progress_callback_user_data = const_cast<std::stop_token *>(&request.stop);
            llama_model * raw = llama_model_load_from_file(path.c_str(), params);
            if (!raw) {
                if (request.stop.stop_requested()) return std::unexpected(Error::cancelled("llama.cpp generation cancelled"));
                return std::unexpected(Error::configuration("llama.cpp could not load GGUF model '" + path + "'"));
            }
            model = SharedModel(raw, llama_model_deleter{});
            impl_->models.emplace(path, model);
        }
    }
    return LlamaStream::create(std::move(model), request);
}

} // namespace jeeves
