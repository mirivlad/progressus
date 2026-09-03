# ADR-0014 — Client save slots and atomic world replacement

Status: **Accepted**

## Context

The simulation already owns versioned save format v1 and can reconstruct sparse authoritative state without serializing untouched generated chunks. That persistence contract should remain independent from Bevy, filesystem conventions, or a particular desktop UI.

Prototype 01 still needs a user-facing save/load path that does not expose `Simulation` internals, corrupt an existing slot during overwrite, or partially replace the running world when a save is invalid.

## Decision

`progressus-app` exposes immutable `save_json`, validated `from_save_json`, and `save_metadata` operations over the simulation persistence contract. Loading constructs and validates a new `Application` first; the client swaps its authoritative application only after both decode and an initial detached snapshot succeed.

The native client provides three bootstrap save slots. Slot files are readable JSON named `slot-1.json` through `slot-3.json` under the platform user-data location for Progressus. Linux prefers `$XDG_DATA_HOME/progressus/saves` and otherwise `~/.local/share/progressus/saves`; Windows uses the local application-data directory; macOS uses `~/Library/Application Support/Progressus/saves`. A current-working-directory fallback exists only when no platform user directory can be resolved.

Slot overwrite uses a temporary file plus a recoverable backup. The new bytes are written and synced first, the previous slot is moved to `.bak`, and only then is the temporary file installed as the slot. On a later scan, a missing slot with a surviving backup is repaired automatically. The file layer lives in the client and introduces no dependency into `progressus-app` or `progressus-sim`.

The shared modal framework exposes a localized Saves window with three slots, seed/tick/save-version metadata, Save and Load actions, and the resolved directory. Russian remains the default UI language. Loading clears client selection/manual tool state, resets transient interpolation/tick scheduling, recenters the camera, and invalidates presentation-window revisions so the loaded authoritative world is fully reconciled. Zoom remains presentation-only and is preserved.

## Consequences

Save format versioning remains an authoritative simulation concern while slot count, file paths, overwrite policy, and desktop UI remain client concerns. Another future client can store the same save bytes in a different way without changing simulation code.

A malformed or unsupported save cannot partially mutate the running game. Save/load currently has three manual slots only: autosave rotation, named colonies, cloud synchronization, thumbnails, compression, migration between unsupported save versions, and background I/O remain future work.
