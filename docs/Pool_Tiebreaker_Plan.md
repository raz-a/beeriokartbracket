# Pool results & tiebreaker plan (next task)

Status: **planned, not yet implemented.** This is the next chunk of work on the
pools stage. Captured here so it travels with the repo.

## Goal

Turn the finished pool into a clean **top-16 cut**, running a secondary
tiebreaker pool when the 16th/17th boundary is tied. The rules
(`docs/Rules_Brackets.md`) say the top 16 total scores advance to the bracket and
**ties are broken by an extra Vanilla race** — this is how we implement that.

## The flow

1. **Main pool** runs to completion with *all* participants (the existing
   bucket-based `Pool` in `src/pool.rs` — 9 buckets / 8 rounds, 6/7/8-player
   races).
2. At the end, call something like `get_pool_results(16)` on the pool. It returns
   the **locked-in top players** together with the **tiebreaker contenders** and
   how many seats they're competing for (see `PoolCut` below). This is the
   evolution of today's `get_top_players`.
3. If there are contenders (boundary tie), build a **second, tiebreaker pool**
   from just those contenders:
   - **One bucket only** (no multi-round bucket progression).
   - **Drop the 6/7/8 race-size rule** — the contender group is whatever size it
     is and races together.
   - Races are **Vanilla** (per the rules).
4. After the tiebreaker races, take the **top `open_slots`** racers from that
   second pool. Those fill the remaining seats; `locked ∪ tiebreak_winners` is
   the final 16.

## The cut result shape

From the design discussion, the cut is best modeled as an enum so the two
outcomes are explicit (matches the "make illegal states unrepresentable" style
used elsewhere in the crate):

```rust
pub enum PoolCut {
    /// Exactly the requested number advance — no tie on the line.
    Decided(Vec<ParticipantId>),
    /// `locked` advance outright; `contenders` race for `spots` seats.
    NeedsTiebreak {
        locked: Vec<ParticipantId>,
        contenders: Vec<ParticipantId>,
        spots: usize,
    },
}
```

Cut logic (already worked out; lives where `get_top_players` is now):

- Sum `Placement::points()` per participant across `completed_races`
  (`None` placement ⇒ 0).
- Sort descending by score.
- `effective_rank = min(rank, len)`; `cutoff_score` = score at
  `effective_rank - 1` (via `NonZero` to guard `rank == 0` / empty).
- `locked` = score **>** cutoff; `contenders` = score **==** cutoff.
- `spots = effective_rank - locked.len()`.
- `contenders.len() == spots` ⇒ `Decided` (tied group fits exactly, e.g. also the
  "fewer players than rank" case). Otherwise ⇒ `NeedsTiebreak`.
- `contenders.len() >= spots` always holds, so it's only ever "fits exactly" vs
  "over-subscribed".

## Open questions / decisions for next time

- **Reusing `Pool` for the tiebreaker.** The current `FillingBucket` /
  `DrainingBucket` / `RaceGroupTracker` bake in the 6/7/8 floor. Options: a
  relaxed constructor, a separate single-bucket/single-race path, or a small
  shared "run a set of racers as one race" primitive. Decide how much to reuse
  vs. add.
- **More than 8 contenders.** A single race maxes at `MAX_RACERS = 8`. If the tie
  block is larger, "one bucket, one race" can't hold everyone — does the
  tiebreaker itself need multiple races (and therefore its own scoring), or is a
  large tie block handled differently?
- **Recursive ties.** The tiebreaker race can itself tie for the last `open_slots`
  seat(s) (esp. since `Race::set_placement` allows duplicate placements). Decide
  whether to require strict placements, recurse into another tiebreaker, or error.
- **Determinism of `locked` ordering.** Membership is deterministic, but tied
  ordering within `locked` comes from `HashMap` iteration. If bracket seeding
  reads that order, add a stable secondary sort key
  (`b.score.cmp(&a.score).then(a.id.cmp(&b.id))`).
- **Beerio penalty.** Scoring still uses raw placement points; the
  finish-before-drink ⇒ forced-8th override isn't modeled on a race result yet.
  Standings aren't final until that exists.

## Pointers

- Cut / scoring: `src/pool.rs` (`get_top_players`, to become `get_pool_results`
  returning `PoolCut`).
- Pool machinery: `src/pool.rs` (`Pool`, `FillingBucket`, `DrainingBucket`,
  `RaceGroupTracker`).
- Rules: `docs/Rules_Brackets.md` (pools bucket qualifier + top-16 + tiebreak).
