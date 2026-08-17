# Custom models (OpenAI-compatible & others) — no rebuild needed

grok-build already lets you point it at any model provider purely from
`~/.grok/config.toml` (`$GROK_HOME/config.toml`). No source changes, no recompile.
You define a **provider** (base URL + how to authenticate + which API backend) and
one or more **models** that use it, then pick a default.

- `api_backend`: `chat_completions` (OpenAI-compatible, the common case),
  `responses` (OpenAI Responses / xAI), or `messages` (Anthropic).
- `env_key`: the environment variable(s) holding the API key — the key never has
  to live in the file. First set, non-blank value wins if you list several.
- `base_url`: used verbatim; grok appends the path (e.g. `/chat/completions`), so
  give the `…/v1` root.
- BYOK isolation: a model on a custom `base_url` never receives your x.ai session
  token — only the credential you configure for it.
- Select the default with `[models] default = "<id>"`, or per run `grok -m <id>`,
  or `GROK_DEFAULT_MODEL=<id>`.

## OpenAI

```toml
[model_providers.openai]
base_url = "https://api.openai.com/v1"
env_key = "OPENAI_API_KEY"
api_backend = "chat_completions"

[model.gpt-4o]
model = "gpt-4o"              # the id sent to the provider
model_provider = "openai"
context_window = 128000

[models]
default = "gpt-4o"
```

```sh
export OPENAI_API_KEY=sk-...
```

## OpenRouter (one endpoint → hundreds of models, incl. Claude/Llama/Gemini)

The simplest way to get *many* models at once: one OpenAI-compatible gateway.

```toml
[model_providers.openrouter]
base_url = "https://openrouter.ai/api/v1"
env_key = "OPENROUTER_API_KEY"
api_backend = "chat_completions"

[model.or-claude]
model = "anthropic/claude-sonnet-4"
model_provider = "openrouter"
context_window = 200000

[model.or-llama]
model = "meta-llama/llama-3.3-70b-instruct"
model_provider = "openrouter"
context_window = 128000
```

## Local (Ollama / llama.cpp / LM Studio — OpenAI-compatible servers)

```toml
[model_providers.local]
base_url = "http://localhost:11434/v1"   # Ollama's OpenAI-compatible endpoint
env_key = "OLLAMA_API_KEY"               # any non-empty value; local servers ignore it
api_backend = "chat_completions"

[model.llama-local]
model = "llama3.3"
model_provider = "local"
context_window = 128000
```

## Anthropic (native Messages API)

`api_backend = "messages"` targets Anthropic's API directly. Anthropic requires an
`anthropic-version` header; add it via `extra_headers` if the native path needs it.
If in doubt, use OpenRouter above (Claude via the chat-completions path) — fewer moving parts.

```toml
[model_providers.anthropic]
base_url = "https://api.anthropic.com/v1"
env_key = "ANTHROPIC_API_KEY"
api_backend = "messages"

[model_providers.anthropic.extra_headers]
anthropic-version = "2023-06-01"

[model.claude]
model = "claude-opus-4-8"
model_provider = "anthropic"
context_window = 200000
```

## Notes

- One provider can back many models; a model field always overrides the provider
  default (e.g. a per-model `base_url` or `api_key`).
- The `sdd` tool and the whole SDD loop are model-agnostic — they drive whatever
  model grok is configured to use, so this composes with the loop unchanged.
- Deeper personalization that *does* need a Rust change (e.g. a bundled default
  provider, a `~/.grok/models.json` merge, per-model auth-scheme tweaks) is
  optional polish — see the fork's roadmap. For just "use more models," this file
  is all you need.
```
