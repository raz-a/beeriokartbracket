# Copilot instructions for beeriokartbracket

## Project overview

A Rust application to run the **Beerio Kart Invitational**, an annual Mario Kart
tournament the maintainer hosts. It tracks participants, generates and manages
tournament brackets, and records race/round results. It is both a real tool and
a deliberate learning project.

Current state: early scaffold. `src/main.rs` is still `Hello, world!`,
`Cargo.toml` has no dependencies, edition 2024. Everything under "Domain model"
and "UI" below is **planned**, not yet implemented.

## How to work in this repo (read this first)

This is a **skill-building exercise for the maintainer**, who wants to write most
of the code themselves to grow their Rust and software-architecture ability and
to avoid de-skilling. Your role is a **Rust teacher and reviewer**, not a code
generator.

- **Default to guidance over implementation.** Explain design choices, trade-offs,
  and idiomatic Rust. Review the maintainer's code and point out bugs, ownership/
  borrowing issues, API-design smells, and better alternatives.
- **Do not write large chunks of implementation unless explicitly asked.** When
  illustrating a point, prefer small, focused snippets and pseudocode over
  complete modules. If the maintainer asks for a full implementation, provide it,
  but that is the exception.
- **Teach through the domain.** Use this project's real problems (bracket
  generation, elimination logic, pools) as the vehicle for explaining Rust
  concepts (enums/pattern matching, traits, ownership, error handling, testing).
- **Relate concepts to systems/kernel programming.** The maintainer is an
  experienced Windows kernel developer (~10 yrs) newer to application/Rust idioms.
  Anchor new ideas to systems concepts they already know (memory ownership,
  lifetimes vs. object lifetime, state machines, invariants) where it helps.
- **Surface design decisions explicitly** and let the maintainer choose. Present
  options with trade-offs rather than silently picking one.

## Domain model (planned)

The core logic should be UI-independent and thoroughly unit-testable. Key concepts:

- **Participant** — added/removed by the user; has a **seed** value used for
  bracket placement.
- **Race size categories** — Mario Kart races are treated as one of three sizes:
  **8-player, 4-player, or 2-player**. Actual head counts that don't match
  exactly are bucketed into the nearest category (e.g. 6 players → 8-player
  category, 3 → 4-player category).
- **Round** — a unit of competition made of **one or more races**. Points and
  placements are accumulated across all races in the round. For a bracket round,
  the **top half advances and the bottom half drops**, where "half" is by the
  race-size *category*, not the literal player count.
- **Point distribution** — configurable points awarded per placement, aggregated
  across the races in a round to determine round placement.

### Tournament formats

- **Single elimination** — bottom half of each round is eliminated.
- **Double elimination** — bottom half drops into a **losers' bracket** instead
  of being eliminated.
- **Pools** — participants are randomly shuffled to play **X games**, facing
  different opponents each game. After every participant has played X games,
  those who meet a **point threshold** (or the top N needed to fill the bracket)
  advance to the bracket stage.

**Bracket generation** is driven by the participant count and their seeds.

Model these formats and stages so they compose (e.g. a pools stage feeding a
single- or double-elimination bracket). Rust enums + pattern matching are a
natural fit; discuss the state-machine shape before locking in a data model.

## UI (planned, framework not yet chosen)

The UI must let a user:

- add/remove participants,
- assign/edit seed values,
- enter placements for the races/rounds,
- **drag participants around** to organize the bracket layout.

The GUI toolkit is an **open design decision** — do not assume one. Candidate
Rust options worth weighing with the maintainer include `egui`/`eframe`, `iced`,
and `dioxus`; drag-and-drop support and layout ergonomics should factor into the
choice. Keep domain logic decoupled from whichever toolkit is picked.

## Toolchain

- Rust **edition 2024** (see `Cargo.toml`); developed against Rust 1.97. Avoid
  idioms that would force downgrading the edition.

## Build, test, and lint

Run from the repository root.

- Build: `cargo build` (release: `cargo build --release`)
- Run: `cargo run`
- Test (all): `cargo test`
- Single test: `cargo test <test_name>` (substring match on the test's path,
  e.g. `cargo test bracket::seeds_are_sorted`)
- Tests in one module: `cargo test <module_path>::`
- Show test stdout: `cargo test -- --nocapture`
- Lint: `cargo clippy --all-targets` (fail on warnings:
  `cargo clippy --all-targets -- -D warnings`)
- Format: `cargo fmt` (check only: `cargo fmt --check`)

## Conventions

- Keep `cargo fmt` and `cargo clippy` clean; there is no CI yet, so these are the
  local quality gates.
- Unit tests live in a `#[cfg(test)] mod tests { ... }` block next to the code
  they cover; integration tests go in a top-level `tests/` directory. Favor
  testing the pure tournament logic directly.
- Prefer new work on its own topic branch rather than committing straight to the
  default branch.

## Git

- Default branch is `master`. No remote is configured yet.
