# Tournament save/load plan

## Goals

- Persist the authoritative, committed tournament state in human-readable JSON.
- Resume any tournament phase, including completed results and the current active race.
- Support a future read-only web viewer without exposing Rust implementation details.
- Recover cleanly from an application crash or accidental exit.
- Keep filesystem and desktop concerns out of the domain model.

## Product decisions

- Files use pretty-printed JSON and the `.bk.json` suffix.
- A new tournament starts on a New/Open/recent-files screen.
- Creating a tournament asks for its name first, then asks for its file path.
- The chosen path must be confirmed before replacing an existing file.
- The tournament name is editable during registration. Renaming does not rename the file.
- The start screen shows up to ten recent files with their name, path, and update time. Missing entries are removed.
- There is no Save or Save As command. Every successful tournament mutation autosaves immediately to the current path.
- Open and New first autosave the current tournament, then replace it. If that save fails, the navigation is aborted.
- Successful saves use a small persistent `Saving...` / `Saved` indicator rather than a popup.
- Registration configuration remains a GUI draft until Start. Unsubmitted placement inputs and other GUI-only state are also drafts and are not persisted.
- A crash can therefore lose the current unsubmitted form or uncommitted registration configuration, but not an earlier successful autosave.

## Ownership boundary

The library owns:

- the versioned snapshot schema;
- conversion between `Tournament` and the snapshot;
- safety-critical validation during snapshot import;
- JSON encoding and decoding errors that describe invalid tournament data.

The GUI owns:

- native New/Open dialogs;
- the current file path;
- recent-file metadata;
- autosave timing and status;
- temporary files, backup rotation, and recovery discovery;
- exit and recovery dialogs;
- clearing and rebuilding transient edit buffers after load.

This keeps `Tournament` independent of paths and desktop APIs while ensuring that the domain, rather than the GUI, defines valid persisted state.

## File schema

Use an explicit snapshot DTO rather than serializing internal structs directly or storing an action log. The top-level document contains at least:

```json
{
  "schema_version": 1,
  "application_version": "0.1.0",
  "name": "4th Annual Northwestern Beerio Kart Invitational",
  "created_at": "...",
  "updated_at": "...",
  "tournament": {}
}
```

The tournament snapshot contains:

- configuration;
- participants with simple integer file IDs and explicit stable ordering;
- the current phase;
- phase-specific state needed to resume;
- completed and active races, including rulesets and optional placements;
- bracket structure, feeders, resolutions, and gauntlet state where applicable.

Participant, race, and bracket-set references use simple integer IDs scoped to one file. Import builds fresh `slotmap` keys and remaps every reference. Participant ordering must be preserved because it is the final pool qualification tiebreaker.

The format supports the current schema version only. Unknown schema versions are rejected clearly. Harmless unknown JSON fields may be ignored, but required fields and safety-critical invariants are validated.

## Load validation

Before replacing the active tournament, import into a separate value and reject snapshots with:

- missing or duplicate IDs;
- references to nonexistent participants, races, or bracket sets;
- invalid or impossible placements;
- racer counts outside the phase rules;
- malformed phase-specific state;
- zero values where `NonZero` configuration is required;
- an active phase whose required state is absent.

Loading must never install a partially converted tournament. Derived standings do not need exhaustive tamper detection in v1, but the loaded state must be safe for all existing domain and view methods.

## Pool randomness after load

Completed pool history and the current active race are preserved. Future pool opponents are allowed to differ from an uninterrupted run after loading. The mutable internals of `StdRng` must not become part of the public JSON contract; loading initializes fresh future-draw state.

## Autosave transaction

Autosaves are synchronous because tournament files are small and action ordering matters.

After a successful in-memory mutation:

1. Build and serialize a complete snapshot.
2. Write it to a sibling temporary file.
3. Flush and close the temporary file.
4. Parse and validate the temporary snapshot.
5. Preserve the previous valid primary as one backup.
6. Replace the primary with the new snapshot using a Windows-safe replacement operation.
7. Update the saved indicator and recent-file metadata.

The app is not required to force directory metadata or cached file contents to physical media; sudden power-loss durability is out of scope.

The chosen commit model updates memory before writing. A process crash in that small interval can lose the newest action. Once a complete temp snapshot exists, startup recovery can offer it even if primary replacement did not finish.

If autosave fails:

- keep the new in-memory state;
- mark it unsaved;
- block further tournament-changing actions;
- offer Retry, Exit Without Saving, and Cancel;
- make Exit Without Saving state exactly which latest changes will be lost.

If the primary file was externally deleted, the next autosave recreates it. External modifications are overwritten by the in-memory tournament.

## Crash recovery

Keep at most:

- the current primary `.bk.json` file;
- one previous known-good backup;
- a sibling temporary snapshot while a save is in progress.

On the start screen and whenever a file is opened, inspect its primary, backup, and temporary candidates. If the primary is invalid, or if a valid temporary snapshot is newer than a valid primary, show a recovery chooser with source and timestamp. Recommend the newest valid candidate, but do not recover it silently.

Do not attempt serialization from a panic hook. Transactional autosave and startup artifact discovery are the recovery mechanisms.

A clean app exit needs no confirmation. If the latest mutation is unsaved because autosave failed, closing offers Retry, Exit Without Saving, or Cancel.

## GUI integration

- Add a start-screen state separate from `TournamentView` with New, Open, and ten recent files.
- Add file lifecycle actions around the existing tournament `Action` handling.
- Autosave only after a domain action succeeds.
- Disable domain-changing controls while a save failure is unresolved.
- On successful load, replace `Tournament` as one operation, copy registration config into the GUI controls when applicable, clear stale status/errors, and rebuild all placement edit maps from the loaded view.
- Show the tournament name and save indicator in the top application chrome.
- Open and New are available after a tournament is open; both save first and return to their respective flow only after success.

## Suggested dependencies

- Library: `serde` with derives and `serde_json`.
- GUI: a native file-dialog crate such as `rfd`.
- Use a small, reviewed file-replacement helper or platform implementation that has defined overwrite behavior on Windows; plain `std::fs::rename` replacement semantics are not sufficient to assume without verification.
- Use an RFC 3339-capable time crate for portable UTC timestamps unless timestamps are represented as documented Unix milliseconds.

## Test plan

Library tests:

- round-trip every tournament phase;
- round-trip DQ and partial active races;
- preserve participant stable ordering;
- reject each dangling-reference class;
- reject invalid placements, counts, configuration, and schema versions;
- ensure failed imports do not alter an existing tournament.

GUI/persistence tests:

- successful save rotates one backup;
- interrupted replacement leaves a recoverable primary, backup, or temp snapshot;
- a newer valid temp snapshot triggers the chooser;
- corrupt candidates are excluded from recovery;
- autosave failure blocks further mutations;
- Open/New abort when the current autosave fails;
- external deletion is recreated on the next save;
- loading clears or rebuilds every transient edit buffer;
- clean exit does not prompt, while failed-save exit does.
