// Code generation target. The generated client lands in ../baml_client
// (repo root) and is git-ignored; regenerate with `uv run baml-cli generate`.
generator target {
    output_type "python/pydantic"
    output_dir "../"
    version "0.223.0"
}

// Reasoning model for the judgment step: Z.AI GLM via the OpenAI-compatible
// Coding Plan endpoint. API key comes from the environment. Model is
// configurable: bump to a newer GLM here without touching code.
//
// The base_url matters: the general endpoint (/api/paas/v4) bills
// pay-as-you-go and answers a Coding Plan key with 429 / code 1113
// ("insufficient balance"). The plan lives under /api/coding/paas/v4.
//
// GLM-5.3 always reasons; depth is set by reasoning_effort ("low" | "high" |
// "max", default "max"). "high" is enough for judging a few dozen precomputed
// numbers and keeps latency/cost sane across the eval suite. The reasoning
// trace comes back in a separate `reasoning_content` field, so BAML's parser
// only ever sees the answer in `content`.
client<llm> Reasoner {
    provider "openai-generic"
    options {
        base_url "https://api.z.ai/api/coding/paas/v4"
        api_key env.ZAI_API_KEY
        model "glm-5.3"
        reasoning_effort "high"
    }
}
