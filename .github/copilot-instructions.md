# Copilot instructions for beeriokartbracket

## Project overview

A Rust application to run the **Beerio Kart Invitational**, an annual Mario Kart
tournament the maintainer hosts. It tracks participants, generates and manages
tournament brackets, and records race/round results. It is both a real tool and
a deliberate learning project. The official rules for the current event live in
`docs/Rules_Brackets.md`; see "Tournament rules" below for the concrete format
the tool must run.

Current state: the end-to-end tournament flow is implemented. The workspace is
split into the UI-independent `beeriokartbracket` library at the repository root
and an `eframe`/`egui` binary in `gui/`. Implemented domain features include the
runtime Registration → Pools → Bracket → Gauntlet → Complete state machine,
participant registration, race scoring, pool buckets and count-back qualifying,
the double-elimination bracket with tiebreaker races, the lives-based gauntlet,
read-only view types, versioned JSON persistence, and unit tests throughout the
domain. The GUI supports tournament setup, participant editing, placement entry
for every competitive phase, phase advancement, and New/Open/Save/Rename file
workflows. Remaining work is refinement rather than scaffolding; notable gaps
include drag-and-drop bracket organization.

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

- **Library + GUI workspace member.** All domain logic lives in the root
  `beeriokartbracket` library (`src/`) and stays UI-independent so it is unit-
  and integration-testable. The `gui/` workspace member owns the `eframe`/`egui`
  application and filesystem dialogs. GUI dependencies belong there, never in
  the library.
- **ID handles over owning references.** Entities live in `slotmap` tables and
  are referenced elsewhere by generational key (e.g. `ParticipantId`), not by
  owned values or `Rc<RefCell<…>>`. This is the "handle table" model: one source
  of truth per entity, IDs held everywhere else. Don't, for example, store a
  `Vec<Race>` on `Participant`; derive such relationships from the race tables.
- **Runtime phase state machine.** `TournamentPhase` is a runtime enum field on
  `Tournament`, deliberately *not* a compile-time typestate, because the
  tournament is a single stored/serializable value driven by user actions.
- **Centralized runtime phase guards.** Public mutations return
  `Result<_, TournamentError>`. Private helpers such as `ensure_registration()`,
  `pools_mut()`, and `bracket_mut()` centralize phase checks and report
  `TournamentError::WrongPhase`; `next_phase()` owns validated transitions.
- **Views separate reads from writes.** The library exposes `TournamentView` and
  phase-specific view types through `Viewable`, while mutation stays on
  `Tournament`. Preserve this UI-independent presentation boundary.
- **Validated, versioned persistence.** `serialize_tournament()` and
  `deserialize_tournament()` use a versioned JSON envelope. Deserialization
  validates IDs and internal phase state before returning a tournament; keep
  persistence migrations and validation in the library.

## Domain model (current)

The core logic should be UI-independent and thoroughly unit-testable. Key concepts:

- **Participant** — a name stored in a `SlotMap` and referenced by
  `ParticipantId`. Participants do not have individual seeds. `Config.seed` is a
  reproducible RNG seed used when shuffling tournament groups.
- **Config** — owns pool-round count, bracket size, races per bracket heat,
  losers'-side gauntlet lives, and RNG seed. Defaults match the concrete event:
  8 pool rounds, 16 qualifiers, 3 races per heat, and 3 losers' lives (winners
  receive twice this value).
- **Race** — up to 8 participant IDs with optional validated placements and a
  `RaceRuleset`. Points are currently derived from placement as 8 down to 1;
  `Placement::DISQUALIFIED` scores zero and has no numeric placement.
- **Race groups** — `RaceGroupTracker` divides a participant count into workable
  groups. Pools and bracket stages apply phase-specific minimum group sizes
  rather than a general-purpose 8/4/2 category enum.
- **Pools** — filling/draining buckets track races completed. The lowest active
  bucket supplies each race, rulesets alternate by bucket, and aggregate scores
  plus count-back placement profiles select the configured bracket field.
- **Bracket** — a double-elimination graph of `BracketSet`s and feeder sources.
  Each set aggregates a configured number of races; unresolved cutoff ties spawn
  a Vanilla tiebreaker race.
- **Gauntlet** — winners start with twice `Config.gauntlet_lives`, losers with
  the configured value. Bottom-half finishers lose a life, and racing continues
  until placements are resolved. The Beerio interval currently reuses the same
  config value (3 by default), a coupling to preserve or separate deliberately.
- **Ruleset** — a per-race axis orthogonal to race size: **Vanilla** or **Beerio
  Kart**. Record the Beerio finish-before-you-drink penalty using the existing
  `Placement::DISQUALIFIED` result selected as `DQ` in the GUI; it does not need
  a separate result field or race type.

### Initial version (v1) assumptions

The broader model in this document is the long-term target. **The first version
deliberately hard-codes the simplifications below.** Treat them as invariants for
now, but keep the code structured so each can be relaxed later without a rewrite
(e.g. don't scatter the literal `8` everywhere — funnel it through one place).

1. **Race grouping is phase-specific.** Pool races are formed as 6-, 7-, or
  8-player groups where possible. Bracket grouping has separate winners' and
  losers' minimums. The broader 8/4/2 category abstraction is not implemented.
2. **Brackets are always double elimination.** Single elimination is not
   selectable in v1 (bottom half always drops into the losers' bracket).
3. **Phases are fixed and linear:** Registration → Pools → Bracket → Gauntlet →
   Complete. Every tournament advances through all five, in that order.

These v1 constraints predate `docs/Rules_Brackets.md`. Treat the rules doc as
authoritative and surface any implementation mismatch rather than silently
changing either behavior or requirements. The **Grand Finals Gauntlet** is the
required bracket endgame, not a conventional four-player final.

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
  their drink is disqualified and receives 0 points**. Tournament operators
  record this with the existing `DQ` placement. The "beer zone" /
  no-drink-and-drive rules are physical and have no data-model impact.

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
- The **top 16 total scores** advance to the bracket. A tie at the qualification
  cutoff is resolved by count-back: compare 1st-place finishes, then 2nd-place
  finishes, continuing through each placement. Identical profiles fall back to
  stable participant ID order.

### Bracket (16 racers, double elimination)

- The top 16 seed a **double-elimination** bracket.
- Each **heat = 3 races**: race 1 Vanilla, race 2 Beerio, race 3 Vanilla. Points
  accumulate across all three — this is the "round = one or more races" concept.
- **Top 4 of the 8** in a heat advance. The other 4 drop to the losers' bracket
  (from a winners' heat) or are eliminated (from a losers' heat).
- Racers tied across the cutoff after 3 races compete in a 4th **Vanilla** race
  for the open advancing places. A tie across that race's cutoff must be
  corrected before the bracket continues.
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

## UI (current)

The GUI is an `eframe`/`egui` native application in `gui/`. It supports creating
and reopening tournament files, autosaving and renaming sessions, editing the
configuration during registration, adding/removing participants, entering or
correcting race placements, viewing standings and bracket progress, and moving
through every tournament phase. A `manual-validation` Cargo feature exposes
development controls for exercising later phases.

Keep GUI state and file-dialog concerns in `gui/`; expose new domain information
through library view types. Drag-and-drop participant organization remains
planned. Participant seed editing is not a current requirement because seeding
uses the tournament-wide RNG seed in `Config`.

## Toolchain

- Rust **edition 2024** (see `Cargo.toml`); developed against Rust 1.97. Avoid
  idioms that would force downgrading the edition.

## Build, test, and lint

Run from the repository root.

- Build: `cargo build --workspace` (release: `cargo build --workspace --release`).
  Bare `cargo build` targets only the GUI (the default member).
- Run the GUI: `cargo run` (the GUI is the default run target).
- Test (all): `cargo test --workspace` (bare `cargo test` targets only the GUI;
  the domain tests live in the library).
- Single test: `cargo test --workspace <test_name>` (substring match on the
  test's path)
- Tests in one module: `cargo test --workspace <module_path>::`
- Show test stdout: `cargo test --workspace -- --nocapture`
- Lint: `cargo clippy --workspace --all-targets` (fail on warnings:
  `cargo clippy --workspace --all-targets -- -D warnings`)
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

- Default branch is `master`.
