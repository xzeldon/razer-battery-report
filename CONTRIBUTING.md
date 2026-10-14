# Contributing to razer-battery-report

The app does one thing: it shows the battery level of a wireless Razer mouse. Changes that keep
it small and reliable are welcome. For anything bigger than a device or a bug fix, open an issue
first so we can agree on it before you write code.

## Building

You need [rustup](https://rustup.rs), which picks up `rust-toolchain.toml`. On Windows that
also means the MSVC build tools.

```sh
cargo run --locked --package razer-battery-report
```

Run these before you open a pull request:

```sh
cargo fmt --all
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Clippy runs with `pedantic` and must stay clean. [`AGENTS.md`](AGENTS.md) describes the
architecture and the code style.

## Adding a mouse

Values come from [openrazer](https://github.com/openrazer/openrazer) whenever it knows the mouse.
Each mode of a mouse (wired, receiver, Bluetooth) has its own USB product id and gets its own
entry.

1. Clone openrazer and list what it knows:

   ```sh
   git clone https://github.com/openrazer/openrazer
   python scripts/list-openrazer-mice.py openrazer --pid 0x00BF
   ```

   Without `--pid` it lists every mouse that reports battery. `charging: unsupported` means the
   mouse cannot tell whether it is charging.
2. Add one entry per product id to `crates/razer-hid/src/catalog.rs`, copying the values the script
   printed, and add the mouse to the table in `README.md`. Run the checks above.
3. If you own the mouse, run `razer-battery-report status` in every mode you added and paste the
   output into the pull request.

If openrazer does not know your mouse, open an issue first. Its values then have to be confirmed
on the mouse itself, and the entry gets a comment saying who tested it and on which connection.

## Pull requests

Open them against `master`. Commits follow
[Conventional Commits](https://www.conventionalcommits.org/) with no body. Release notes are
generated from commit subjects, so write a subject a user would understand.

## AI

Use whatever tools you like, but you must understand, review and test what you submit. PR
descriptions, issues and replies in discussions must be your own words. A generated summary of a
small change is fine, and so is AI proofreading or translation of text you wrote. Device values
must come from openrazer or from tests on the mouse, never from a model's memory. Pull requests that ignore this may be closed
without comment.

## License

razer-battery-report is MIT-licensed, and your contributions are licensed under the same terms.
