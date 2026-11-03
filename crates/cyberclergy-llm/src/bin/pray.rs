//! `pray` — composes one prayer through the shared system prompt.
//!
//! The prayer text goes to stdout, so the output can be piped or diffed; all
//! diagnostics go to stderr.

use std::process::ExitCode;

use cyberclergy_llm::{PrayerGenerator, PrayerSpec, ProviderId, Registry, Settings};

const USAGE: &str = "\
pray - compose one prayer through CyberClergy's shared system prompt

USAGE:
  cargo run -p cyberclergy-llm --bin pray -- [OPTIONS]

OPTIONS:
  --provider <id>     Use one provider instead of the fallback chain
  --model <name>      Override the model for this run
  --tradition <text>  Tradition to pray in (for example \"Roman Catholic\")
  --language <text>   Language of the prayer
  --tone <text>       Tone of the prayer
  --length <text>     Length description, in words
  --intent <text>     What the prayer is about
  --name <text>       Name of the person to pray for
  --verbose           Print provider, model and token usage to stderr
  -h, --help          Show this help

PROVIDERS:
  openrouter, gemini, groq, mistral, cloudflare, nvidia";

#[derive(Debug, Default)]
struct Args {
    help: bool,
    verbose: bool,
    provider: Option<ProviderId>,
    model: Option<String>,
    tradition: Option<String>,
    language: Option<String>,
    tone: Option<String>,
    length: Option<String>,
    intent: Option<String>,
    name: Option<String>,
}

impl Args {
    fn parse<I: Iterator<Item = String>>(mut raw: I) -> Result<Self, String> {
        let mut args = Self::default();
        while let Some(argument) = raw.next() {
            match argument.as_str() {
                "-h" | "--help" => args.help = true,
                "--verbose" => args.verbose = true,
                "--provider" => {
                    let value = next_value(&mut raw, "--provider")?;
                    args.provider = Some(
                        value
                            .parse::<ProviderId>()
                            .map_err(|error| error.to_string())?,
                    );
                }
                "--model" => args.model = Some(next_value(&mut raw, "--model")?),
                "--tradition" => args.tradition = Some(next_value(&mut raw, "--tradition")?),
                "--language" => args.language = Some(next_value(&mut raw, "--language")?),
                "--tone" => args.tone = Some(next_value(&mut raw, "--tone")?),
                "--length" => args.length = Some(next_value(&mut raw, "--length")?),
                "--intent" => args.intent = Some(next_value(&mut raw, "--intent")?),
                "--name" => args.name = Some(next_value(&mut raw, "--name")?),
                other => return Err(format!("unknown argument `{other}`")),
            }
        }
        Ok(args)
    }

    fn spec(&self) -> PrayerSpec {
        let mut spec = PrayerSpec::new();
        if let Some(value) = &self.tradition {
            spec = spec.with_tradition(value.clone());
        }
        if let Some(value) = &self.language {
            spec = spec.with_language(value.clone());
        }
        if let Some(value) = &self.tone {
            spec = spec.with_tone(value.clone());
        }
        if let Some(value) = &self.length {
            spec = spec.with_length(value.clone());
        }
        if let Some(value) = &self.intent {
            spec = spec.with_intent(value.clone());
        }
        if let Some(value) = &self.name {
            spec = spec.with_user_name(value.clone());
        }
        spec
    }
}

fn next_value<I: Iterator<Item = String>>(raw: &mut I, flag: &str) -> Result<String, String> {
    raw.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

#[tokio::main]
async fn main() -> ExitCode {
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

    let generator = PrayerGenerator::new(registry);
    let spec = args.spec();
    let mut request = match generator.build_request(&spec) {
        Ok(request) => request,
        Err(error) => {
            eprintln!("could not render the system prompt: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Some(model) = &args.model {
        request = request.with_model(model.clone());
    }

    let outcome = match args.provider {
        Some(provider) => generator.registry().chat(provider, &request).await,
        None => generator.registry().chat_with_fallback(&request).await,
    };

    match outcome {
        Ok(response) => {
            if args.verbose {
                print_metadata(&response);
            }
            println!("{}", response.text);
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("prayer generation failed: {error}");
            ExitCode::FAILURE
        }
    }
}

fn print_metadata(response: &cyberclergy_llm::ChatResponse) {
    let usage = response
        .usage
        .as_ref()
        .map(|usage| {
            format!(
                "tokens {}/{}/{}",
                optional_count(usage.prompt_tokens),
                optional_count(usage.completion_tokens),
                optional_count(usage.total_tokens)
            )
        })
        .unwrap_or_else(|| "tokens unknown".to_string());
    eprintln!(
        "[{} | {} | {} ms | {}]",
        response.provider,
        response.model,
        response.latency.as_millis(),
        usage
    );
}

fn optional_count(value: Option<u32>) -> String {
    value.map_or_else(|| "?".to_string(), |count| count.to_string())
}
