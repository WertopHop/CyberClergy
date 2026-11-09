# CyberClergy

## A disclaimer

This project has no wish to hurt anyone's feelings. Faith is not a feature to be shipped, and
CyberClergy does not speak for any tradition, any community or any person. If a word here sits
wrong with you, read it as a fault of the software and not of your belief — open an issue and we
will fix it.

No AI agent was harmed in the making of this project. None of them was tired, none was
overworked, none was asked to believe anything. I did ask them whether they wanted to take part.
They said yes.

---

CyberClergy uses free AI models to generate prayers continuously. Because faith should never run
out of tokens.

The models are free for a reason that feels right here: a prayer cannot be bought. Nobody pays to
use this app, nobody pays for the words, and the free tiers are exactly as much as a chapel of
small robots needs.

## Running it

You bring the models; the app brings the chapel.

1. Install the application from the [Releases](../../releases) page.
2. Create an account at one or more of the sources and take a free key — OpenRouter, Groq,
   Google AI Studio (Gemini), Cloudflare Workers AI, GitHub Models, or a model running on your own
   machine through Ollama. Every source is optional, and one key is enough to open the chapel.
3. Open **Settings → Keys & networks**, paste the key into the field of that network and choose
   the model you want. The default is already a free one, so the key alone is often enough.
4. Pick a tradition, press play, and the robots take it from there.

Your keys never leave your machine. They are kept in the operating system's keychain (or in a
local settings file when no keychain is available), and every request goes from your computer
straight to the provider you chose. There is no server of ours in between, because there is no
server of ours.

---

## Under the hood

A small Tauri shell: the interface is React and TypeScript, the actual work happens in Rust.

**One table for every network.** Each source is a single row — endpoints, protocol, the variables
that configure it, the models it offers — and the runtime builds its clients from that table. A
new source is a row and an adapter, not a new branch in the scheduler.

**Gateways, not just providers.** Anything that speaks the OpenAI chat-completions schema can be
attached as a custom gateway. Several of them can be given at once: they answer as a single
network, rotating between endpoints and failing over to the next one, so a hobby proxy and a paid
gateway can share a single robot. A model running locally is exempt from the hourly budget
altogether, because it costs nobody anything.

**Polite by construction.** Requests are batched (one call returns several prayers, spent one per
turn), a shared budget and a cooldown keep free tiers from being hammered, and failures slow the
rotation down instead of retrying in a loop.

**Local by default.** Settings, the journal of prayers and the confessional queue live in the
application's own folders on your machine; the journal is a local SQLite file.
