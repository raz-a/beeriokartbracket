# Tournament save/load plan

## Priorities

1. Keep the implementation simple.
2. Resume the authoritative tournament state in any phase.
3. Recover the previous successful save after an app crash.
4. Keep file handling out of tournament behavior as far as practical.

Power-loss durability, long-term file compatibility, manual JSON editing, recent
files, and a stable public interchange format are out of scope for v1.

## User experience

- Files are pretty JSON with the `.bk.json` suffix.
- The first screen has only **New** and **Open**.
- New asks for the tournament name, then a path. Replacing an existing file
  requires confirmation.
- The name can be edited during registration without renaming the file.
- There are no Save or Save As commands. Each successful domain action performs
  a synchronous autosave.
- A small indicator shows `Saving...`, `Saved`, or `Save failed`.
- Open and New save the current tournament first. They stop if that save fails.
- A clean exit needs no prompt. After a save failure, exit offers Retry, Exit
  Without Saving, or Cancel.
- Registration configuration, unsubmitted placements, and other GUI-only fields
  remain drafts and are not saved.

## Small architecture

### Library: serialization only

Add one `persistence` module containing a versioned document wrapper:

```rust
struct TournamentDocument {
    schema_version: u32,
    name: String,
    tournament: Tournament,
}
```

The module exposes only JSON encode/decode operations and a persistence error.
It performs no filesystem I/O.

Serialize the existing private domain state directly with serde instead of
creating parallel DTOs and remapping every ID. This requires `Serialize` and
`Deserialize` derives on the private state types, but adds no save/load behavior
to those modules.

Enable slotmap's `serde` feature. Its implementation round-trips slot contents,
generations, and keys, so participant, race, and bracket-set references retain
their identity and ordering without a second ID system.

Replace `StdRng` with rand's `Xoshiro256PlusPlus` and enable rand's `serde`
feature. It is already publicly provided by rand 0.10, supports `SeedableRng`,
and serializes its small state. A loaded pool then produces the same future
draws as an uninterrupted pool without custom RNG handling.

The document supports only the current schema version. A future incompatible
domain change increments that version and may reject old files.

### Load safety

Serde handles syntax, required fields, enum variants, `NonZero` values, and
slotmap's own representation. Keep custom validation narrow:

- deserialize `Placement` through `Placement::new` so invalid values cannot be
  constructed from JSON;
- reject references to participants or bracket sets absent from their slotmaps;
- reject obviously impossible racer counts and phase structure.

Validation helpers should remain private and live beside the types whose
invariants they inspect. The persistence module calls one top-level tournament
validation method before returning a loaded value. Do not build a second model
or recompute every derived result merely to detect hand-edited files.

### GUI: one file module

Add one `gui/src/persistence.rs` module. It owns:

- the current path and tournament name;
- New/Open dialogs;
- synchronous save and load;
- one backup file;
- the saved/failed status;
- automatic backup recovery.

The existing GUI action handler remains the only place domain mutations happen.
After an action succeeds, it calls the file module once. Loading replaces the
`Tournament` in one operation and then clears or rebuilds transient edit maps.

Do not split file lifecycle, recovery, and recent-file tracking into separate
modules in v1.

## Save algorithm

There is no temporary file.

1. Serialize the complete document in memory. A serialization error leaves disk
   untouched.
2. If the primary exists and decodes successfully, copy it over
  `<name>.bk.json.bak`. If it is invalid, preserve the existing backup.
3. Create/truncate the primary and write the new JSON.
4. Flush the Rust writer and report `Saved`.

No filesystem sync or platform-specific atomic replacement API is required.

If the app exits or crashes during step 3, the primary may be incomplete. Open
then tries the primary first and the backup second. When the backup is used, the
GUI reports that the previous save was recovered and restores it as the primary
without first overwriting the valid backup.

This deliberately guarantees recovery of the previous successful save, not the
newest in-memory action. A crash between mutation and completed overwrite can
lose that newest action. A crash during the first-ever save has no earlier file
to recover; the New flow should create its initial valid file before entering
registration.

If a normal save call fails, keep the changed tournament in memory, mark it
unsaved, and block further domain mutations until Retry succeeds or the user
exits without saving. If the primary was externally deleted, the next save
recreates it. External edits are overwritten.

Do not write from a panic hook.

## Dependencies

Library:

- `serde` with derive support;
- `serde_json`;
- `slotmap` with its `serde` feature;
- `rand` with its `serde` feature.

GUI:

- one native file-dialog crate such as `rfd`.

No time, UUID, atomic-file, or application-settings dependency is needed.

## Focused tests

Library:

- JSON round-trip in registration, pools, bracket, gauntlet, and complete;
- preserve IDs, DQ, partial active races, and the next pool draw;
- reject the wrong schema version, invalid placements, and dangling references.

GUI file module:

- a normal save keeps one previous backup;
- a corrupt primary automatically loads and restores the valid backup;
- a failed save blocks further mutations;
- successful load resets transient GUI edit state;
- the initial New save exists before registration opens.
