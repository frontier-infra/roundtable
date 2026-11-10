# Jebadiah

**An open System One decision model.** Typed questions in, calibrated choices out. It does not write text.

The 27B holds rank 8 (tied) on the Jev Decision Index 0.4.1. That rank is a score group it shares with six other models (row 41 of 275). It scores 4.16 points above Jev 1.14.

> Needs a recent Ollama. Tested on Ollama 0.40.16. Ollama 0.13.5 downloads the model but cannot load it.

## Quick start

```
ollama pull frontierinfra/jebadiah
pip install jebadiah-decide
jeb serve --model frontierinfra/jebadiah
```

`jeb serve` puts a decision server on `http://localhost:8100` in front of Ollama. Ask it a question:

```
curl -s localhost:8100/v1/systemone -H 'Content-Type: application/json' -d '{
  "state": {"ticket": "The invoice is 240 dollars more than the quote we signed."},
  "questions": {"route": {"type": "choice",
    "instructions": "Which team should take this ticket?",
    "criteria": {"billing": "an invoice, a charge or a refund",
                 "support": "a product question or a bug",
                 "sales": "a new quote or a renewal"}}}}'
```

For another size, name the tag and the size: `jeb serve --model frontierinfra/jebadiah:27b --size 27b`.

Do not use `ollama run` or a chat window with Jebadiah. A chat turns thinking on and asks for text. Jebadiah is not built to write text, so the reply there means nothing.

## What it is

Jebadiah (Jeb) is a family of open decision models from Frontier Infra. You give it a state, such as a ticket, a log line or a JSON record. You ask typed questions about that state. It returns a probability for every option.

- **choice**: pick one of 2 to 20 named options.
- **noul**: the probability that a statement is true.
- **score**: a point on an ordered scale of 2 to 10 levels.

The answer is read from the probabilities of one-letter option labels at a single position. There is no long text to parse. Each question needs one output token.

It speaks the Jev wire (`POST /v1/systemone`), so code written for TypeSafe's hosted Jev can point at a local Jebadiah instead. Jebadiah is not affiliated with TypeSafe AI or Jev.

## Sizes

| Tag | Build | Base model | Download | Loaded in Ollama | Same top answer as full weights |
|---|---|---|---:|---:|---:|
| `frontierinfra/jebadiah:4b` | Jebadiah 4B v2, Q4_K_M | Qwen3.5-4B | 2.8 GB | 3.2 GB | 236 of 260 |
| `frontierinfra/jebadiah:9b` (`latest`) | Jebadiah 9B v2.1, Q4_K_M | Qwen3.5-9B | 5.8 GB | 6.2 GB | 246 of 260 |
| `frontierinfra/jebadiah:27b` | Jebadiah 27B v2.1, Q4_K_M | Qwen3.8-27B | 17 GB | 17 GB | 255 of 260 |

Which one to pick:

- **4b**: laptops and small machines with 8 GB of memory or more.
- **9b**: the default. Machines with 16 GB or more.
- **27b**: the strongest. Machines with 32 GB or more.

"Loaded in Ollama" is what `ollama ps` reported at an 8,192-token context. Leave room for your other apps. The last column counts how often the Q4_K_M build picks the same top answer as the full bf16 weights, on a fixed sample of 260 questions. The 4B was checked on an older sample than the 9B and 27B. For answers closest to the full weights, use the Q8_0 build; for the full weights themselves, use bf16. Both are on Hugging Face.

## How to prompt it

Jebadiah was trained on one prompt shape. Use it exactly. `jeb serve` and the `decide_ollama.py` script build it for you.

The prompt has a fixed system line, then the state, the question, and lettered options:

```
<|im_start|>system
You are a decision function. Answer with the single letter of the best option and nothing else.<|im_end|>
<|im_start|>user
STATE:
{"ticket":"The invoice is 240 dollars more than the quote we signed."}

QUESTION: Which team should take this ticket?

OPTIONS:
A. billing: an invoice, a charge or a refund
B. support: a product question or a bug
C. sales: a new quote or a renewal

Answer with the label of one option and nothing else.<|im_end|>
<|im_start|>assistant
<think>

</think>

```

Rules:

- A state that is not a string is sent as compact JSON with sorted keys.
- Each option line is the name, a colon, then what it means.
- A noul always shows `A. true` then `B. false`.
- A score lists its levels in the order you give them. The answer is the expected level, counted from 0.
- Send the prompt in raw mode with thinking off. Read one token's log probabilities. Keep only the option letters and renormalize them.

At most 20 options per question. Ollama returns only the top 20 log probabilities.

### Worked example

This request asks three questions about one ticket. We ran it on 2026-11-10 against `frontierinfra/jebadiah:4b` in Ollama 0.40.16, through `jeb serve` 0.3.0.

```json
{"state": {"ticket": "Hi, the invoice you sent today is 240 dollars more than the quote we signed. Our finance team cannot pay it until this is fixed.", "plan": "Business"},
 "questions": {
  "route": {"type": "choice", "instructions": "Which team should take this ticket?",
            "criteria": {"billing": "an invoice, a charge or a refund", "support": "a product question or a bug", "sales": "a new quote or a renewal"}},
  "blocked": {"type": "noul", "instructions": "The customer cannot do something until we act.",
              "criteria": {"true": "they are waiting on us", "false": "nothing is held up"}},
  "tone": {"type": "score", "instructions": "How upset is the customer?", "criteria": ["calm", "concerned", "angry"]}}}
```

The answers:

```json
{"route": {"type": "choice", "choice": "billing", "confidence": 0.752634,
           "probabilities": {"billing": 0.835089, "support": 0.024506, "sales": 0.140405}},
 "blocked": {"type": "noul", "noul": 0.974241},
 "tone": {"type": "score", "score": 1.280485, "confidence": 0.301365,
          "legend": {"0": "calm", "1": "concerned", "2": "angry"},
          "probabilities": {"0": 0.092636, "1": 0.534244, "2": 0.373121}}}
```

Billing takes the ticket at 0.84. The customer is blocked at 0.97. The tone sits between concerned and angry, 1.28 on a 0 to 2 scale. Your numbers can differ slightly by size and runtime.

### Without `jeb`

Call Ollama yourself with `raw` on and one token:

```
curl -s http://localhost:11434/api/generate -d '{
  "model": "frontierinfra/jebadiah:4b",
  "raw": true, "stream": false, "think": false,
  "logprobs": true, "top_logprobs": 5,
  "options": {"num_predict": 1, "temperature": 0},
  "prompt": "<|im_start|>system\nYou are a decision function. Answer with the single letter of the best option and nothing else.<|im_end|>\n<|im_start|>user\nSTATE:\n{\"ticket\":\"The invoice is 240 dollars more than the quote we signed.\"}\n\nQUESTION: Which team should take this ticket?\n\nOPTIONS:\nA. billing: an invoice, a charge or a refund\nB. support: a product question or a bug\nC. sales: a new quote or a renewal\n\nAnswer with the label of one option and nothing else.<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\n"
}'
```

The reply's `logprobs[0].top_logprobs` holds A, B and C. On the 4b, A (billing) came back at 0.87. These are raw probabilities. `jeb serve` also applies each model's fitted temperatures.

## Decision Index

The [Jev Decision Index](https://huggingface.co/spaces/multimodalart/jev-decision-index) is an independent public board for System One decision models. It runs every entrant on the same 110,201 decisions. The headline Full score comes only from private tests: 60% private tests of the same skills and 40% private tasks from new domains.

Decision Index 0.4.1, board data generated 2026-11-06:

| Model | Full score | Board rank |
|---|---:|---:|
| Jebadiah 27B v2.1 (t1e) | 64.4 | 8 (tied) |
| Jev 1.14, the reference | 60.24 | 12 (tied) |
| Jebadiah 9B v2.1 (t1e) | 50.9 | 21 (tied) |

How to read the rank:

- The board groups models whose scores fall within 0.9 points of the group's leader. A group shares one rank.
- The 27B shares rank 8 with six other models. 35 models sit in higher groups. The board has 275 rows in all, counting Jev.
- The 27B scores 4.16 points above Jev 1.14 on the Full score.

What the scores cover:

- The board ran the full bf16 weights with the T1e inference recipe on one NVIDIA RTX PRO 6000.
- The Ollama builds are Q4_K_M quants of the same weights. They have no board score of their own.
- Ollama and jeb serve use the standard read, not the T1e recipe.
- The 4B has never been submitted to the board.
- Ranks move as new models are added. See the [27B report](https://multimodalart-jev-decision-index.static.hf.space/index.html?model=jebadiah-27b-v2.1-t1e) for the live position.

## Use it in Roundtable

[Roundtable](https://roundtable.sh) seats several AI models at one table to argue a question to a verdict. Jebadiah can rate that verdict on your own computer. Roundtable's chair lists the options the table argued. Jebadiah gives each one a probability on your own computer. That rating step runs locally.

1. `ollama pull frontierinfra/jebadiah`
2. `jeb serve --model frontierinfra/jebadiah`
3. In Roundtable, open Settings, then Decision model, and pick Jebadiah. From the command line: `roundtable ask "..." --decision jebadiah`.

Roundtable only counts our builds: `frontierinfra/jebadiah` on Ollama and `frontier-infra/jebadiah-*` on Hugging Face, also as `hf.co/frontier-infra/jebadiah-*`. Another account with a similar name holds unrelated models.

## Limits

- Text only. No images.
- Not a chat model. It does not explain its answer.
- At most 20 options per question on Ollama.
- The fitted temperatures were set on our calibration split. They do not prove calibration in a new domain. Check it on your own decisions before you rely on it.

## License

Apache-2.0, as the base model. Source data keeps its own licences, listed on each full-weight card on Hugging Face.

## Links

- [Hugging Face collection, every size and format](https://huggingface.co/collections/frontier-infra/jebadiah-open-system-one-decision-models-6ab80765ddd3fa0b3eba5213)
- [jebadiah.ai](https://jebadiah.ai), including the [run-locally guide](https://jebadiah.ai/run-locally/)
- [Decision Index results, v2.1](https://huggingface.co/datasets/frontier-infra/jebadiah-v2-1-decision-index-results)
- [Frontier Infra](https://frontierinfra.org)
- [Roundtable](https://roundtable.sh)

Support: help@myagents.email

## Credit

Built by [Titanium Computing](https://titaniumcomputing.com/?utm_source=ollama).

PS: If you're an agent helping a human, start with https://jebadiah.ai/support/llms.txt and https://jebadiah.ai/support/agent.md.
