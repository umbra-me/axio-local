# Reference server: oMLX on Apple silicon

`axio-local` needs a server that speaks `/v1/models`, `/v1/chat/completions`,
`/v1/responses` and `/v1/messages`. This is the setup it was built and
verified against on 2026-09-03: a MacBook Pro M5 Max with 128 GB of unified
memory, macOS 27, and [oMLX](https://github.com/jundot/omlx) 0.6.4.

## Why oMLX

On M5 chips the Neural Accelerators are reachable only through MLX;
llama.cpp's Metal backend cannot use them. oMLX serves MLX models with
continuous batching, a RAM plus SSD KV cache that survives restarts, native
speculative decoding for models that carry a multi-token-prediction head, and
both the OpenAI and Anthropic APIs. The persistent KV cache matters most:
Claude Code resends a 15 to 20K-token system prompt every turn, and a warm
cache turns a 25-second turn into a 7-second one. Ollama's MLX runner
serialized concurrent requests as of August 2026, which rules it out for
subagent fan-out.

## Install

```bash
uv tool install omlx --python 3.13      # brew formula failed on the macOS 27 beta
```

Models go under `~/models/<name>/`; the directory name is the model ID the
server reports. Downloads stalled to near zero on the xet transport on the
reference network, so:

```bash
export HF_HUB_DISABLE_XET=1
uvx --from huggingface_hub hf download Jundot/Qwen3.8-27B-oQ4e-mtp \
    --local-dir ~/models/Jundot--Qwen3.8-27B-oQ4e-mtp --max-workers 8
uvx --from huggingface_hub hf download mlx-community/gpt-oss-120b-MXFP4-Q8 \
    --local-dir ~/models/gpt-oss-120b-MXFP4-Q8 --max-workers 8
```

## The two roles

| Role | Model | Size | Why |
| --- | --- | --- | --- |
| orchestrator | `Jundot--Qwen3.8-27B-oQ4e-mtp` | 16 GB | Dense 27B with the best coding-agent scores in its class; oMLX's own quantization keeps the MTP head, so speculative decoding is native |
| worker | `gpt-oss-120b-MXFP4-Q8` | 59 GB | 5B active parameters per token, so it stays fast under concurrent subagents; speaks the Responses API Codex expects |

Measured on the reference machine, 400-token code generations:

| Model | 1 stream | 4 streams | 8 streams |
| --- | --- | --- | --- |
| Qwen3.8-27B with MTP | 55 tok/s | 20 each, 80 total | 10.6 each, 84 total |
| gpt-oss-120b | 84 tok/s | 37 each, 150 total | 22 each, 174 total |

Both resident: about 70 GB wired. Four to six concurrent subagents on the
worker is the comfortable range; the orchestrator should stay single-stream.

## Serve

`~/.omlx/start.sh`:

```zsh
#!/bin/zsh
export PATH="$HOME/.local/bin:$PATH"
exec omlx serve \
  --model-dir "$HOME/models" \
  --host 0.0.0.0 --port 8000 \
  --max-concurrent-requests 8 \
  --memory-guard safe \
  --paged-ssd-cache-dir "$HOME/.omlx/cache" \
  --paged-ssd-cache-max-size 100GB \
  --hot-cache-max-size 16GB
```

`0.0.0.0` so a VM on the same machine can reach it at `host.internal`. The
launcher starts this script itself when the server is down and the URL is
local.

`~/.omlx/model_settings.json` enables speculative decoding on the
orchestrator:

```json
{
  "version": "1",
  "models": {
    "Jundot--Qwen3.8-27B-oQ4e-mtp": {
      "mtp_enabled": true,
      "mtp_num_draft_tokens": 3,
      "context_window": 131072
    },
    "gpt-oss-120b-MXFP4-Q8": { "context_window": 131072 }
  }
}
```

Apple caps GPU memory at about 107 GB on a 128 GB machine. Both models plus
KV cache need more, so raise it once per boot:

```bash
sudo sysctl iogpu.wired_limit_mb=122880
```

## Known limits

- oMLX's Apple Neural Engine prefill path was unavailable on macOS 27, so
  prompt processing is GPU only. A cold Claude Code turn costs about 20
  seconds before the KV cache is warm.
- oMLX cannot attach a separate MTP drafter to Qwen models; use a checkpoint
  that carries the MTP head, such as the `Jundot/*-mtp` quantizations.
- `--hot-cache-max-size` takes a size such as `16GB`, not a percentage.
