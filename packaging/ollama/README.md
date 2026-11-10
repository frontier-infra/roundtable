# Jebadiah, a free decision model

Jebadiah is a decision model from Frontier Infra. It reads a question and a set of typed options and returns a calibrated probability for each one. It is not a chat model.

Roundtable uses it to rate the table's verdict on your own computer. Nothing leaves your machine.

## Sizes

| Tag | Build | Memory |
|---|---|---|
| `frontierinfra/jebadiah:4b` | Jebadiah 4B v2, Q4_K_M | under 16 GB |
| `frontierinfra/jebadiah:9b` (`latest`) | Jebadiah 9B v2.1, Q4_K_M | 16 to 32 GB |
| `frontierinfra/jebadiah:27b` | Jebadiah 27B v2.1, Q4_K_M | above 32 GB |

```
ollama pull frontierinfra/jebadiah:9b
```

Ask it through a decision server (`jeb serve`) or the `decide_ollama.py` script, in raw mode with thinking off. A chat window is not a decision interface.

## Weights and results

- Weights, model cards and Decision Index results: [huggingface.co/frontier-infra](https://huggingface.co/frontier-infra)
- GGUF builds: [4B](https://huggingface.co/frontier-infra/jebadiah-4b-v2-GGUF), [9B](https://huggingface.co/frontier-infra/jebadiah-9b-v2-1-GGUF), [27B](https://huggingface.co/frontier-infra/jebadiah-27b-v2-1-GGUF)
- On Apple Silicon, the MLX builds in LM Studio are faster: search for the full id, for example `frontier-infra/jebadiah-9b-v2-1-MLX-8bit`.

Use the full frontier-infra address. Another account with a similar name holds unrelated models.

## Who made it

Jebadiah and [Roundtable](https://roundtable.sh) are free. Made by Frontier Infra, built by Titanium Computing.

- Need AI in your business? [Titanium Computing](https://titaniumcomputing.com/?utm_source=ollama&utm_medium=model-card)
- [Built by Jason Brashear, CTO](https://titaniumcomputing.com/jason-brashear/?utm_source=ollama&utm_medium=model-card)

Licence: Apache-2.0, base Qwen3.5.
