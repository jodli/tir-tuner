// Code generation target. The generated client lands in ../baml_client
// (repo root) and is git-ignored; regenerate with `uv run baml-cli generate`.
generator target {
    output_type "python/pydantic"
    output_dir "../"
    version "0.223.0"
}

// Reasoning model for the judgment step. API key comes from the environment.
// Model is configurable: bump to a newer Claude model here without touching code.
client<llm> Reasoner {
    provider anthropic
    options {
        api_key env.ANTHROPIC_API_KEY
        model "claude-sonnet-4-20250514"
        temperature 0
    }
}
