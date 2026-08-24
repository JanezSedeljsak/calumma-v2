# docs/

Every prose doc for Calumma lives here. Only `README.md`, `AGENTS.md`, and `CLAUDE.md`
stay at the repository root — the root README is the public front page, and the other two
are the agent entry points.

| File | Answers |
| --- | --- |
| [`AGENTS.md`](../AGENTS.md) | Architecture, product rules, STRICT SCOPE — read it first |
| [`FLOW.md`](FLOW.md) | What the product does: screens, canvas, shortcuts, persistence, I/O |
| [`STYLE.md`](STYLE.md) | Design system expanded — tokens, hierarchy, spacing, motion |
| [`ENGINE.md`](ENGINE.md) | How the Rust crates fit together and why each boundary is where it is |
| [`RENDERING.md`](RENDERING.md) | The frame loop, dirty flags, and the pan/zoom performance strategy |

`todo.md` and `plans/` also live here and are **gitignored on purpose** — a local working
backlog, not something the repo or the app ships. See [`docs/plans/README.md`](plans/README.md)
for that convention.
