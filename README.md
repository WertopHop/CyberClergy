# CyberClergy

CyberClergy uses free AI models to generate prayers continuously. Because faith should never run out of tokens.

## Status

Stages 0-2 are complete: the provider layer (six optional free-tier backends with
fallback, retries and typed errors) and the shared prayer prompt pipeline. The
chapel interface, religion profiles and the continuous prayer scheduler come next.

## Repository layout

| Path | Purpose |
|------|---------|
| `crates/cyberclergy-llm/` | Provider layer, prompt rendering, `probe` and `pray` CLIs |
| `crates/cyberclergy-llm/resources/prompts/system_prompt.md` | The single system prompt every model receives |
| `src-tauri/` | Tauri v2 desktop shell (owns provider state, will own key storage) |
| `src/` | React + TypeScript frontend |
| `design/` | Logo source used to generate the icon set |
| `.env.example` | Template with every supported variable (no secrets) |

## Requirements

- Node.js 20 or newer
- Rust 1.80 or newer (stable toolchain)
- WebView2 runtime (preinstalled on Windows 10/11)

## Getting started

```bash
cp .env.example .env      # then fill in at least one provider key
npm install
npm run tauri dev
```

Every provider is optional. A provider without credentials is simply disabled,
so you can start with one personal free-tier key and add more later.

## Composing prayers

All networks go through one funnel: the system prompt in
[`system_prompt.md`](crates/cyberclergy-llm/resources/prompts/system_prompt.md),
rendered by `PromptTemplate`, wrapped into a request by `build_prayer_request`,
then sent through the provider registry (or a chain of them).

Edit the prompt file directly — its instructions start on the first line, so
there is no code to touch. Available placeholders:

| Placeholder | Meaning | Default |
|-------------|---------|---------|
| `{{tradition}}` | Religion or denomination the prayer belongs to | inclusive, non-denominational wording |
| `{{language}}` | Language of the prayer | English |
| `{{tone}}` | Tone of voice | reverent and warm |
| `{{prayer_length}}` | Length description | one short paragraph, 60-110 words |
| `{{intent}}` | What the prayer is about | general wellbeing and protection |
| `{{user_name}}` | Name of the person served | "the person you serve" |

Rendering is strict: an unknown or unterminated placeholder fails loudly instead
of sending a half-filled prompt to a paid endpoint.

Generate a prayer from the terminal:

```bash
# One prayer through the fallback chain, text on stdout
cargo run -p cyberclergy-llm --bin pray

# Pick a tradition and an intention, show provider metadata on stderr
cargo run -p cyberclergy-llm --bin pray -- \
  --provider openrouter \
  --tradition "Sunni Islam" \
  --intent "peace for a worried family" \
  --verbose
```

Useful flags: `--tradition`, `--language`, `--tone`, `--length`, `--intent`,
`--name`, `--model`, `--provider`, `--verbose`, `--help`.

## Verifying keys without launching the UI

```bash
# Show which providers are configured and which variables are missing
cargo run -p cyberclergy-llm --bin probe -- --list

# Send a connectivity check to one provider
cargo run -p cyberclergy-llm --bin probe -- --provider groq "ping"
```

## Supported providers

| Provider | Required variables | Free-tier notes |
|----------|--------------------|-----------------|
| OpenRouter | `OPENROUTER_API_KEY` | use models with the `:free` suffix |
| Google AI Studio (Gemini) | `GEMINI_API_KEY` | free tier has daily request limits |
| Groq | `GROQ_API_KEY` | fast free tier with per-minute limits |
| Mistral AI | `MISTRAL_API_KEY` | free experimental tier |
| Cloudflare Workers AI | `CLOUDFLARE_ACCOUNT_ID`, `CLOUDFLARE_API_TOKEN` | token needs the Workers AI permission |
| NVIDIA NIM | `NVIDIA_API_KEY` | build.nvidia.com credits |

Each provider also accepts `<PROVIDER>_MODEL` to pin a specific model and
`<PROVIDER>_BASE_URL` to override the endpoint.

Application-wide settings: `CC_PROVIDER_ORDER`, `CC_HTTP_TIMEOUT_SECS`,
`CC_MAX_RETRIES`, `CC_DEFAULT_TEMPERATURE`, `CC_DEFAULT_MAX_TOKENS`.

`CC_DEFAULT_MAX_TOKENS` defaults to 2048 because reasoning models spend part of
the budget on hidden reasoning before writing any text. A prayer that comes back
empty now reports whether the budget ran out instead of failing silently.

## Adding a provider

Providers are data-driven. The table in
[`config.rs`](crates/cyberclergy-llm/src/config.rs) (`PROVIDER_SPECS`) describes
identity, protocol, variable names, default URL and default model.

- **OpenAI-compatible backend** (the common case): add a `ProviderId` variant and
  one `PROVIDER_SPECS` row with `Protocol::OpenAiChat`. No adapter code, because
  OpenRouter, Groq, Mistral and NVIDIA NIM already share one implementation.
- **New wire protocol**: add a `Protocol` variant, a module under
  [`providers/`](crates/cyberclergy-llm/src/providers/mod.rs) implementing the
  `Provider` trait, and one arm in `Registry::with_client`.

A unit test fails if a `ProviderId` has no spec row, so the table cannot drift.

## Tests

```bash
cargo test -p cyberclergy-llm      # adapters covered with HTTP mocks
cargo clippy -p cyberclergy-llm --all-targets -- -D warnings
npm run build                      # type-checks and builds the frontend
```

## Privacy

Free tiers often reserve the right to use submitted content for model
improvement. Anything sent to a provider leaves the machine; keep that in mind
before the prayer scheduler starts running continuously.
