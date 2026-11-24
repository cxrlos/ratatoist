# Cache the account in one JSON file

ratatoist keeps everything it synced, with the sync token and the Todoist user id, in one compact JSON file, `cache.json` in the cache folder, so a start paints my tasks before the network answers and then asks Todoist only for what changed. Without it every start is a full sync behind a loading screen, and full syncs are the expensive, rate-limited read. The store is already a set of serde types merged from sync responses, so writing it out as it is costs one `serde_json` call and no schema of its own: a schema version in the file, bumped when the store's shape changes, makes an older file a cache miss and a full sync instead of a migration. The file is written atomically (a temp file, then a rename) with mode 0600, after a burst of merges settles and on quit, and never holds the API token.

## Considered options

- **SQLite**: rejected. It would bring a C dependency, a schema and migrations to keep a few thousand rows that are always read whole at startup and replaced whole on a full sync. Row-level writes only pay off with partial reads, which the app does not do.
- **One file per resource**: rejected. Separate writes can leave the files out of step with each other and with the sync token, and one atomic rename cannot cover them.
- **Pretty-printed JSON**: rejected. The file is never read by hand, and indentation grows a large account's file for nothing.

## Consequences

The whole store is serialized on each write, so a very large account writes a few megabytes every few seconds while changes stream in; the debounce bounds that, and a single file is fine at that size. A store with a command Todoist has not answered is not written, so the cache never holds a write that may be rolled back. A cache of another user (the first sync, or the user fetched after it, has a different user id), from another schema version, with no user or sync token, or that does not parse is discarded for a full sync, and `R` replaces it too. `sync_state.json` in the config folder, which held only the token, is deleted after the first sync: a token without its data cannot paint anything. `--demo` and the Not logged in screen read and write no cache.
