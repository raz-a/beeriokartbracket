# Copilot instructions for beeriokartbracket

## Project overview

A Rust application to run the **Beerio Kart Invitational**, an annual Mario Kart
tournament the maintainer hosts. It tracks participants, generates and manages
tournament brackets, and records race/round results. It is both a real tool and
a deliberate learning project. The official rules for the current event live in
`docs/Rules_Brackets.md`; see "Tournament rules" below for the concrete format
the tool must run.

Current state: past the initial scaffold. The crate is split into a library
(`src/lib.rs`, all domain logic) and a thin binary (`src/main.rs`) — see
"Architecture (current)" below. `Cargo.toml` is edition 2024 and depends on
`slotmap`. Implemented so far: the `Tournament` top-level type, a runtime
`TournamentPhase` state machine (Registration → Pools → Bracket → Gauntlet →
Complete), a
`Registration` capability handle for phase-gated participant editing, and
`Participant` (name + seed) stored in a `SlotMap` keyed by `ParticipantId`. Still
**planned, not yet implemented**: races, pools, bracket/rounds, point
distribution, tests, and the entire UI. A `#![allow(dead_code)]` (with a TODO) is
in place while the model is incomplete.

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

## Architecture (current)

Decisions already made and reflected in the code — build on these rather than
re-deriving or second-guessing them:

- **Library + thin binary.** All domain logic lives in the `beeriokartbracket`
  library (`src/lib.rs`) and stays UI-independent so it is unit- and
  integration-testable. `src/main.rs` is a thin shell that drives the library.
  Any GUI toolkit belongs in the binary's dependencies, never the library's.
- **ID handles over owning references.** Entities live in `slotmap` tables and
  are referenced elsewhere by generational key (e.g. `ParticipantId`), not by
  owned values or `Rc<RefCell<…>>`. This is the "handle table" model: one source
  of truth per entity, IDs held everywhere else. Don't, for example, store a
  `Vec<Race>` on `Participant`; derive such relationships from the race tables.
- **Runtime phase state machine.** `TournamentPhase` is a runtime enum field on
  `Tournament`, deliberately *not* a compile-time typestate, because the
  tournament is a single stored/serializable value driven by user actions.
- **Capability handles for phase-gated operations.** Rather than a phase check in
  every method, `Tournament` hands out a borrow-scoped handle (e.g.
  `registration() -> Option<Registration<'_>>`) that only exists in the
  correct phase, so the check lives in one place. Transitions consume the handle
  (`Registration::start(self)`) so using it after a transition is a compile
  error.

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
  across the races in a round to determine round placement. The concrete v1
  default (8-player race) is 8 points for 1st down to 1 for 8th; see "Tournament
  rules".
- **Ruleset** — a per-race axis orthogonal to race size: **Vanilla** or **Beerio
  Kart**. Only Beerio's finish-before-you-drink penalty affects scoring; see
  "Tournament rules".

### Initial version (v1) assumptions

The broader model in this document is the long-term target. **The first version
deliberately hard-codes the simplifications below.** Treat them as invariants for
now, but keep the code structured so each can be relaxed later without a rewrite
(e.g. don't scatter the literal `8` everywhere — funnel it through one place).

1. **Race sizes are fixed.** Every non-bracket-endgame race is an **8-player**
   race; the general 8/4/2 bucketing described above is not exercised in v1. The
   bracket's endgame is **not** a single 4-player final — it is the lives-based
   **Grand Finals Gauntlet** exactly as specified in `docs/Rules_Brackets.md`
   (see "Tournament rules").
2. **Brackets are always double elimination.** Single elimination is not
   selectable in v1 (bottom half always drops into the losers' bracket).
3. **Phases are fixed and linear:** Registration → Pools → Bracket → Gauntlet →
   Complete. Every tournament advances through all five, in that order.

These v1 simplifications predate `docs/Rules_Brackets.md`. Where the concrete
rules go beyond a simplification (the pools bucket qualifier, 6–8-player pool
races), treat the rules doc as authoritative and confirm with the maintainer how
much of it v1 should actually implement. The **Grand Finals Gauntlet** is not
optional — it is the correct, intended bracket endgame.

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

## Tournament rules (authoritative: docs/Rules_Brackets.md)

`docs/Rules_Brackets.md` holds the official rules for the **4th Annual
Northwestern Beerio Kart Invitational** and is the source of truth for the
concrete event format. The abstract model above is the long-term generalization;
the rules below are what the tool must actually run. When they conflict, prefer
the rules doc and surface the difference to the maintainer.

### Rulesets (a per-race axis)

Every race uses one of two **rulesets**, orthogonal to race size:

- **Vanilla Mario Kart** — standard race (150cc, recommended items/laps, random
  course).
- **Beerio Kart** — Vanilla plus drinking rules. The only rule that touches
  *scoring* is the penalty: **a racer who finishes the race before completing
  their drink is forced to 8th place (1 point)**. Model this as a per-result
  override, not a distinct race type. The "beer zone" / no-drink-and-drive rules
  are physical and have no data-model impact.

### Point distribution (concrete default)

8-player placement points, 1st → 8th: **8, 7, 6, 5, 4, 3, 2, 1**. This is the
concrete instance of the "configurable point distribution" concept — keep it
configurable, ship these as the default.

### Pools (bucket qualifier)

Not the generic "everyone plays X games" model — a specific bucket algorithm:

- **9 buckets**, indexed by *races completed* (0 through 8); everyone starts in
  bucket 0 after registration.
- Repeatedly draw racers from the **lowest non-empty bucket** for a single race
  (**6-, 7-, or 8-player** depending on registration count), award points by
  placement, then move those racers up one bucket.
- **Even buckets use Beerio, odd buckets use Vanilla.**
- Continue until every racer is in the "8 races" bucket.
- The **top 16 total scores** advance to the bracket. Ties are broken by an
  extra **Vanilla** race.

### Bracket (16 racers, double elimination)

- The top 16 seed a **double-elimination** bracket.
- Each **heat = 3 races**: race 1 Vanilla, race 2 Beerio, race 3 Vanilla. Points
  accumulate across all three — this is the "round = one or more races" concept.
- **Top 4 of the 8** in a heat advance. The other 4 drop to the losers' bracket
  (from a winners' heat) or are eliminated (from a losers' heat).
- A tie after 3 races is broken by a 4th **Vanilla** race.
- Runs until **4 racers remain in each of the winners' and losers' brackets** (8
  total), who feed the Grand Finals Gauntlet.

### Grand Finals Gauntlet

A lives-based elimination, not a single 4-player final:

- Starting **lives**: winners'-bracket racers get **6**, losers'-bracket racers
  get **3**.
- Back-to-back races; racers finishing in the **bottom half (rounding up)** lose
  **1 life**. Zero lives ⇒ eliminated.
- Once **4 racers remain**, races move to a single screen.
- **Every 3rd race uses Beerio**; the rest are Vanilla.
- Continues until **one racer remains** (the champion).

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
