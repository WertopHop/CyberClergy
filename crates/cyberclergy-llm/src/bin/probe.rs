//! `probe` — verifies provider credentials from `.env` without launching the UI.
//!
//! Usage:
//!   cargo run -p cyberclergy-llm --bin probe -- --list
//!   cargo run -p cyberclergy-llm --bin probe -- --provider groq "ping"
//!
//! Exit code is non-zero when at least one requested provider fails, which makes
//! the tool usable from scripts and CI smoke checks.

use std::process::ExitCode;

use cyberclergy_llm::{ChatRequest, ProviderId, Registry, Settings};

const DEFAULT_PROMPT: &str = "Reply with exactly one short sentence confirming you are online.";
const USAGE: &str = "\
probe - verify CyberClergy provider credentials

USAGE:
  cargo run -p cyberclergy-llm --bin probe -- [OPTIONS]

OPTIONS:
  --list                 Show every provider and whether its credentials are set
  --provider <id>        Probe one provider (repeatable or comma-separated)
  --model <name>         Override the model for this run
  --prompt <text>        Prompt to send (default: connectivity check)
  -h, --help             Show this help

PROVIDERS:
  openrouter, gemini, groq, mistral, cloudflare, nvidia

Without --provider every configured provider is probed in CC_PROVIDER_ORDER.";

struct Args {
    help: bool,
    list: bool,
    providers: Vec<ProviderId>,
    model: Option<String>,
    prompt: String,
}

impl Args {
    fn parse<I: Iterator<Item = String>>(mut raw: I) -> Result<Self, String> {
        let mut args = Self {
            help: false,
            list: false,
            providers: Vec::new(),
            model: None,
            prompt: DEFAULT_PROMPT.to_string(),
        };
        while let Some(argument) = raw.next() {
            match argument.as_str() {
                "-h" | "--help" => args.help = true,
                "--list" => args.list = true,
                "--provider" => {
                    let value = raw
                        .next()
                        .ok_or_else(|| "--provider requires a value".to_string())?;
                    for part in value.split(',') {
                        let id = part
                            .trim()
                            .parse::<ProviderId>()
                            .map_err(|error| error.to_string())?;
                        args.providers.push(id);
                    }
                }
                "--model" => {
                    args.model = Some(
                        raw.next()
                            .ok_or_else(|| "--model requires a value".to_string())?,
                    );
                }
                "--prompt" => {
                    args.prompt = raw
                        .next()
                        .ok_or_else(|| "--prompt requires a value".to_string())?;
                }
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(args)
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    // A missing `.env` is the normal case for CI and packaged builds.
    let _ = dotenvy::dotenv();

    let args = match Args::parse(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("error: {message}\n\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    if args.help {
        println!("{USAGE}");
        return ExitCode::SUCCESS;
    }

    let settings = Settings::from_env();
    if args.list {
        print_statuses(&settings);
        return ExitCode::SUCCESS;
    }

    let registry = match Registry::new(&settings) {
        Ok(registry) => registry,
        Err(error) => {
            eprintln!("could not initialise the provider registry: {error}");
            return ExitCode::FAILURE;
        }
    };
    if registry.available().is_empty() {
        eprintln!(
            "no providers are configured - copy `.env.example` to `.env` and fill in at least one key"
        );
        return ExitCode::FAILURE;
    }

    let targets = if args.providers.is_empty() {
        registry.available()
    } else {
        args.providers.clone()
    };

    let mut failures = 0usize;
    for id in targets {
        let mut request = ChatRequest::user(args.prompt.clone());
        if let Some(model) = args.model.clone() {
            request = request.with_model(model);
        }
        match registry.chat(id, &request).await {
            Ok(response) => print_success(&response),
            Err(error) => {
                failures += 1;
                eprintln!("[fail] {id}: {error}");
            }
        }
    }

    if failures == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn print_statuses(settings: &Settings) {
    let rows: Vec<(&'static str, String, String)> = settings
        .statuses()
        .into_iter()
        .map(|status| {
            let state = if status.configured {
                "configured".to_string()
            } else {
                format!("missing: {}", status.missing_vars.join(", "))
            };
            (status.id.as_str(), state, status.model)
        })
        .collect();

    // Width follows the longest status so the table stays aligned as providers
    // gain or lose required variables.
    let status_width = rows
        .iter()
        .map(|(_, state, _)| state.chars().count())
        .max()
        .unwrap_or(10)
        + 2;

    println!("{:<12} {:<status_width$} MODEL", "PROVIDER", "STATUS");
    for (id, state, model) in rows {
        println!("{id:<12} {state:<status_width$} {model}");
    }
}

fn print_success(response: &cyberclergy_llm::ChatResponse) {
    let usage = response
        .usage
        .as_ref()
        .map(|usage| {
            format!(
                "tokens {}/{}/{}",
                usage
                    .prompt_tokens
                    .map_or_else(|| "?".to_string(), |v| v.to_string()),
                usage
                    .completion_tokens
                    .map_or_else(|| "?".to_string(), |v| v.to_string()),
                usage
                    .total_tokens
                    .map_or_else(|| "?".to_string(), |v| v.to_string())
            )
        })
        .unwrap_or_else(|| "tokens unknown".to_string());
    println!(
        "[ok]   {} - {} - {} ms - {}",
        response.provider,
        response.model,
        response.latency.as_millis(),
        usage
    );
    for line in response.text.lines() {
        println!("       {line}");
    }
}
