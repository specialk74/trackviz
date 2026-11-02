# Fonts

The `.ttf` files are not in git. Both fonts are embedded in the executable at build time:

- **Inter** (variable font, SIL OFL 1.1, see `LICENSE-Inter.txt`): `build.rs` downloads it
  with `curl` on the first build, from a URL pinned to a commit of the Google Fonts repo,
  and checks its SHA-256.
- **DejaVu Sans Mono** (DejaVu license, see `LICENSE-DejaVu.txt`): comes from the
  `dejavu` crate on crates.io.

**Offline builds:** download the Inter file by hand (the URL is in `build.rs`) and save it
here as `fonts/Inter.ttf`. If it is present, `build.rs` uses it instead of downloading.
