//! `pray` — composes one prayer through the shared system prompt.
//!
//! The prayer text goes to stdout, so the output can be piped or diffed; all
//! diagnostics go to stderr.

use std::process::ExitCode;

use cyberclergy_llm::{
    PrayerGenerator, PrayerSpec, ProfileCatalog, ProviderId, Registry, Settings,
};

const USAGE: &str = "\
pray - compose one prayer through CyberClergy's shared system prompt

USAGE:
  cargo run -p cyberclergy-llm --bin pray -- [OPTIONS]

OPTIONS:
  --religion <id>     Pray in a built-in religion profile
  --list-religions    Show the available religion profiles and exit
  --short             Use the compact length the scheduler uses
  --provider <id>     Use one provider instead of the fallback chain
  --model <name>      Override the model for this run
  --tradition <text>  Override the tradition description
  --language <text>   Language of the prayer
  --tone <text>       Tone of the prayer
  --length <text>     Length description, in words
  --intent <text>     What the prayer is about
  --name <text>       Name of the person to pray for
  --verbose           Print provider, model and token usage to stderr
  -h, --help          Show this help

PROVIDERS:
  openrouter, gemini, groq, mistral, cloudflare, nvidia

RELIGIONS:
  See --list-religions; profiles live in resources/clergy/<id>/.";

#[derive(Debug, Default)]
struct Args {
    help: bool,
    verbose: bool,
    short: bool,
    list_religions: bool,
    religion: Option<String>,
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
                "--short" => args.short = true,
                "--list-religions" => args.list_religions = true,
                "--religion" => args.religion = Some(next_value(&mut raw, "--religion")?),
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

    /// Profile defaults first, then explicit CLI overrides on top.
    fn spec(&self, catalog: &ProfileCatalog) -> Result<PrayerSpec, String> {
        let base = match &self.religion {
            Some(id) => {
                let profile = catalog.get(id).ok_or_else(|| {
                    format!(
                        "unknown religion `{id}` (available: {})",
                        catalog.ids().join(", ")
                    )
                })?;
                profile.prayer_spec_for_length(self.short)
            }
            None => {
                if self.short {
                    return Err("--short requires --religion".to_string());
                }
                PrayerSpec::new()
            }
        };

        let mut spec = base;
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
        Ok(spec)
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

    let catalog = match ProfileCatalog::embedded() {
        Ok(catalog) => catalog,
        Err(error) => {
            eprintln!("could not load the built-in religion profiles: {error}");
            return ExitCode::FAILURE;
        }
    };
    if args.list_religions {
        print_religions(&catalog);
        return ExitCode::SUCCESS;
    }

    let spec = match args.spec(&catalog) {
        Ok(spec) => spec,
        Err(message) => {
            eprintln!("error: {message}");
            return ExitCode::FAILURE;
        }
    };

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

fn print_religions(catalog: &ProfileCatalog) {
    println!(
        "{:<14} {:<18} {:<16} {:<10} DIRECTIONS",
        "ID", "NAME (EN)", "NAME (RU)", "PRAYER"
    );
    for profile in catalog.list() {
        println!(
            "{:<14} {:<18} {:<16} {:<10} every {} s, compact length available with --short",
            profile.id,
            profile.localized_name("en"),
            profile.localized_name("ru"),
            profile.identity.prayer_term,
            profile.schedule.interval_seconds
        );
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
