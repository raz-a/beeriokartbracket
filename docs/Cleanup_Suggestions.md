# Cleanup suggestions (nice-to-have)

Status: **not scheduled.** None of this is required for correctness — the code
works as-is. These are opportunities to trade runtime `assert!`/`expect!` checks
for **compile-time invariants** ("make illegal states unrepresentable"). Captured
here so they travel with the repo; pick them up if/when the surrounding code is
being touched anyway.

Scope: every non-test `assert` / `debug_assert` / `expect` / `unwrap` in `src/`
was reviewed (`src/main.rs`, `src/pool.rs` — no other file has any). Test-module
assertions are intentionally left alone; `unwrap` in `#[cfg(test)]` is the idiom.

## Summary

| Site | Assertion | Liftable to a compile-time invariant? |
|------|-----------|----------------------------------------|
| `pool.rs` `get_results` | `debug_assert!(race.is_complete())` | **Yes** — `CompletedRace` newtype |
| `pool.rs` `get_results` | `.expect("Race is complete…")` on `place` | **Yes** — same newtype (removes both) |
| `pool.rs` `advance` | `.seal().expect("…always succeed")` | **Yes** — validated-count newtype |
| `pool.rs` `advance` | `add_racers(…).expect("…collisions or overflow")` | **Partly** — overflow yes, distinctness awkward |
| `pool.rs` `advance` | `debug_assert!(current_race.is_none())` | No — control-flow fact |
| `pool.rs` `advance` | `debug_assert!(current_bucket.is_empty())` | No — arithmetic invariant |
| `pool.rs` `advance` | `debug_assert_eq!(next_bucket.len(), n)` | No — arithmetic invariant |
| `pool.rs` `get_results` | `debug_assert!(tiebreaker_seats < tied.len())` | No — arithmetic invariant |
| `main.rs` `main` | `r.start().unwrap()` | No — genuine runtime error |

## Worth lifting

### 1. `CompletedRace` newtype → removes both `get_results` checks

Highest ROI. Today `Pool::completed_races` is `Vec<Race>` holding races that are
*complete by convention* — `advance` only pushes after an `is_complete()` check —
but nothing in the type says so, so `get_results` re-asserts completeness and then
re-unwraps every `Option<Placement>`.

Encode the completeness once, at the boundary where a race finishes:

```rust
pub struct CompletedRace {
    racers: Vec<(ParticipantId, Placement)>, // Placement, not Option<Placement>
    ruleset: RaceRuleset,
}

impl Race {
    // Consume a race; succeed only if every racer placed.
    pub fn finish(self) -> Result<CompletedRace, Race> {
        if !self.is_complete() {
            return Err(self);
        }
        let racers = self.racers.into_iter()
            .map(|(id, p)| (id, p.expect("is_complete() guarantees Some")))
            .collect();
        Ok(CompletedRace { racers, ruleset: self.ruleset })
    }
}
```

Then `completed_races: Vec<CompletedRace>` and `get_results` iterates
`(ParticipantId, Placement)` with **no `Option`, no `expect`, no `debug_assert`**.
The completeness check moves from "asserted at every read" to "proven once at
construction" — the handle-table pattern applied to state: validate at the
boundary, hand an opaque validated type inward.

Caveat: one `expect` survives *inside* `finish` (the `Option → Placement`
unwrap), because it checks-then-moves. Still a net win: one obviously-correct
trust point three lines below its own guard, vs. two scattered through the
consumer. (It can be erased entirely by collecting into `Option<Vec<_>>` via
`FromIterator`, at the cost of a clumsier ownership dance on the `Err` path.)

### 2. Validated-count newtype → removes the `seal().expect(...)`

`FillingBucket::seal` returns `Option` only because `RaceGroupTracker::new` can
reject a participant count. But `Pool::new` already validated *that exact count*,
which is invariant across every bucket — so each later `seal` re-checks a proven
fact, hence the `expect` in `advance`.

Capture the proof in a type constructible only through the checked path:

```rust
// Its existence *is* the proof that the count forms legal races.
struct ValidRoster { participants: Vec<ParticipantId> }

impl ValidRoster {
    fn new(participants: Vec<ParticipantId>) -> Option<Self> {
        RaceGroupTracker::new(participants.len())?; // the one and only check
        Some(Self { participants })
    }
}
```

Have `Pool` carry enough to rebuild a `DrainingBucket` infallibly (the roster is
fixed; only membership rotates), so bucket creation stops returning `Option` and
the `expect` has nothing to guard.

## Not worth lifting (leave as-is)

- **`add_racers(...).expect(...)`** — *partly* liftable. The overflow half is
  easy: if `pop_next_race_candidate` returned a bounded `RaceGroup` (e.g. an
  `ArrayVec<_, MAX_RACERS>` or a newtype proven `<= 8`), an infallible
  `Race::from_group` would drop the overflow check. But the **distinctness** half
  ("no duplicate racers") is a property of the source `Vec` (unique slotmap keys +
  `split_off`); encoding "these elements are distinct" in a type needs a set-like
  type or a trust boundary. Not worth the machinery for one `expect`.
- **`current_race.is_none()` / `current_bucket.is_empty()` /
  `next_bucket.len() == n` / `tiebreaker_seats < tied.len()`** — control-flow and
  arithmetic facts (`take()` leaves `None`; tracker counts sum to the participant
  total; `spots <= tied.len()` follows from the sort + cutoff). Expressing these
  in types is dependent-type territory Rust doesn't have. Keep them as
  `debug_assert`s — cheap, compiled out of release, and good executable
  documentation of the proof.
- **`main.rs` `start().unwrap()`** — not an invariant: `start` legitimately fails
  with `NoParticipants`, a runtime condition. The fix is to *handle* the error,
  not lift it. Making "non-empty" a compile-time guarantee would need a
  `Registration` typestate, which contradicts the deliberate choice to keep phases
  a runtime state machine. In real UI code, propagate/report it; in the prototype
  `main`, the `unwrap` is fine.

## Pointers

- Race completeness: `src/race.rs` (`Race`, `Placement`, `is_complete`),
  `src/pool.rs` (`Pool::completed_races`, `get_results`).
- Bucket sealing / counts: `src/pool.rs` (`FillingBucket::seal`,
  `RaceGroupTracker`, `Pool::new`, `Pool::advance`).
