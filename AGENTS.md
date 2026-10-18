# razer-battery-report

A tray app that shows the battery level of wireless Razer mice, plus a CLI in the same binary.
Cargo workspace, edition 2024, resolver 3. Windows first; Linux and macOS come later, so
platform code stays behind one boundary.

## Crate layout

```
crates/
  razer-hid/              library: Razer HID protocol, device catalog, discovery, battery queries
  razer-battery-report/   binary: CLI, tray, notifications, config, logging
```

Dependency direction is strict: `razer-battery-report → razer-hid → hidapi`. `razer-hid` is a
standalone library for other projects too, so it never knows about the tray, config, logging
setup or notifications.

Inside the binary:

- `main.rs` picks the mode: a subcommand or flag runs the CLI, no arguments starts the tray.
- `monitor.rs` is a pure function from readings to events (connected, disconnected, low,
  critical, full). Notification rules live there and nowhere else.
- `platform/` holds every OS-specific call (autostart, console attach, opening a file, the
  clipboard). Code outside `platform/` is OS-agnostic and carries no `#[cfg(target_os)]`.

## Rules

- **Minimalism.** For the user the app does one thing: show the mouse battery. Settings live in
  the tray menu; a new feature needs a reason a normal user would notice.
- **openrazer is the source of truth** for product ids, transaction ids, interface numbers,
  response waits and command bytes (<https://github.com/openrazer/openrazer>, `driver/`). When a
  value here or in a PR disagrees with openrazer, openrazer wins.
  `scripts/list-openrazer-mice.py <checkout>` prints those values for every battery mouse.
- **Devices are hardcoded** in `crates/razer-hid/src/catalog.rs`, one entry per USB product id.
  Adding a mouse is a data-only change: a new entry, never a new `match` arm elsewhere. A mouse
  that openrazer does not know yet may still be added when its values were confirmed on the
  hardware; its entry carries a comment naming who tested it and on which connection. Only mice
  are in scope; keyboards share the protocol and may join later.
- **Catalog fields mirror the script's JSON** (`pid`, `name`, `tid`, `interface`, `wait_us`,
  `charging`, `serial`, `serial_tid`), so a future CI job can diff openrazer against the catalog.
  Rename a field in both places or neither.
- **One physical mouse is one device.** Endpoints are grouped by the serial number the mouse
  reports over the protocol (class `0x00`, id `0x82`). The USB descriptor serial belongs to the
  dongle or cable and is never used for identity.
- **Standard per-user paths only.** Config, logs and any other state go to the OS's standard
  per-user directories, resolved through the `directories` crate.

## Building and checks

```sh
cargo run --locked --package razer-battery-report
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Clippy runs with `pedantic` and stays clean, `--all-targets` included. Lint levels live in the
root `Cargo.toml` under `[workspace.lints]`. A local `#[allow]` sits on the narrowest item and
carries a comment saying why. A complex type gets a module-local `type` alias rather than an
`allow(clippy::type_complexity)`. `unsafe` is allowed only in `platform/`, each block with a
`// SAFETY:` comment.

The release workflow generates `THIRD-PARTY.md`, the licences of every crate in the exe, with
`scripts/generate-notices.py`, passes it to `build.rs` through `THIRD_PARTY_NOTICES` to embed
it in the exe, and attaches it to the release; it is not committed. Other builds embed a
placeholder. A dependency whose licence is not in `about.toml`'s `accepted` list fails CI.

## Code style

- Every type and every function whose behaviour is not obvious from its name carries a `///`
  comment saying what it is for and what a caller has to know. One or two plain sentences.
  Every public item of `razer-hid` is documented (`missing_docs` is on there).
- Module order: `use` block, `const`s, types, impls, private free helpers at the bottom.
- Traits are imported anonymously: `use anyhow::Context as _;`.
- Use let-else and return early. A two-arm choice on a bool is
  `match flag { true => …, false => … }`.
- Magic numbers become module-level `const`s with a name that says what they mean.
- Errors: `razer-hid` returns its own `thiserror` enum, so callers can tell a sleeping mouse
  (`Timeout`) from a real failure. The binary uses `anyhow::Result` with lowercase context:
  `.context("cannot read config")`.
- Logs are prefixed by subsystem and use the same voice:
  `log::warn!("poller: cannot read battery of {name}: {error}")`.
- A poisoned mutex is recovered with `.lock().unwrap_or_else(PoisonError::into_inner)`, never
  unwrapped.
- Dependencies are declared once in the root `[workspace.dependencies]` and pulled in with
  `workspace = true`.

## Tests

Pure code is tested: report encoding and CRC, the catalog's invariants, discovery grouping over a
mock transport, `monitor`, config parsing. Tests are a `#[cfg(test)] mod tests` at the bottom of
the file they cover. Protocol tests use real byte captures as fixtures. Hardware behaviour is
checked by hand on a real mouse; say which mouse and which connection you tested in the PR.

## Commits

Conventional Commits: `type(scope): description`, imperative, lowercase, no trailing period, no
body. Scopes: `hid`, `catalog`, `tray`, `cli`, `config`, `ci`. Pull requests are squash-merged
into `master`, so the PR title becomes the commit subject and follows the same format. Release
notes are built from these subjects, so a `feat` or `fix` subject reads as a line a user
understands. There is no hand-written changelog.

## Releases

Both crates share one version, `[workspace.package] version` in the root `Cargo.toml`. Releases
are made straight on `master`, without a pull request:

1. Check that the tree is clean, `master` is up to date with `origin` and CI passed on its last
   commit.
2. Pick the version from the commit subjects since the last `v*` tag: a breaking change (`!` or
   `BREAKING CHANGE`) bumps major, any `feat` bumps minor, otherwise patch.
3. Set the version, run `cargo check` so `Cargo.lock` follows, commit `chore: release X.Y.Z`.
4. Tag `vX.Y.Z` and push the commit and the tag. The maintainer's git signs tags, so an agent
   stops after step 3 and hands over `git tag vX.Y.Z -m vX.Y.Z` and
   `git push origin master vX.Y.Z` instead of running them.

The release workflow checks the tag against the version, builds the exe and publishes the
release with GitHub's generated notes; a tag with `-` is a pre-release.

## Pull requests

One heading, one list, nothing else:

```markdown
## Summary

- add razer viper v3 pro to the device catalog
- keep showing the last battery level while the mouse sleeps
```

Bullets are lowercase, imperative, one line each, and describe behaviour rather than files.
