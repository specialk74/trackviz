# trackviz

Renders automation scenarios to MP4: a top view of the track (carriers, lanes,
NSD, sampling point, analyzer) next to a live sequence diagram (Interface ↔
Analyzer ↔ DMX), with explanatory captions.

Each scene is a text file in `scenes/`. To make a new video you write a new
scene, not new code.

## Requirements

- Rust (stable). Frames are rasterized in-process with the `resvg` crate.
- `ffmpeg` on PATH (only for MP4 output; `--still` and `--check` don't need it)
  - Debian/Ubuntu: `apt install ffmpeg`
  - macOS: `brew install ffmpeg`
  - Windows: `winget install Gyan.FFmpeg`
- Fonts: none to install, and no `.ttf` in git. Both are embedded in the executable:
  Inter is downloaded by `build.rs` on the first build (needs `curl`, SHA-256 checked),
  DejaVu Sans Mono comes from the `dejavu` crate. For offline builds see `fonts/README.md`.

## Usage

```sh
cargo build --release
cargo test --release      # unit tests (parser, timeline rules, rendering, all scenes)
B=./target/release/trackviz

$B scenes/01_prepare_sample_release.toml --check            # validate + print timeline
$B scenes/01_prepare_sample_release.toml --still 17 -o f.png # one frame at t=17 s
$B scenes/01_prepare_sample_release.toml                    # full video → out/01_prepare_sample_release.mp4
$B scenes/01_prepare_sample_release.toml -o other.mp4        # full video, chosen file name
```

Without `-o` the video goes to `out/<scene file name>.mp4` (`out/` is created if missing).

`--check` prints every step with its start and end time. Use it as the event
log when tuning timings.

## Scene format (TOML subset)

All coordinates are in a fixed 1920×1080 space. The track panel covers
x 30–950 and y 120–885. `[video] width/height` only scales the output.

| Section | Keys |
|---|---|
| `[video]` | `title`, `subtitle`, `width`, `height`, `fps`, `tail` (seconds held at the end), `show_clock`, `show_times` |
| `[[point]]` | `id`, `x`, `y` |
| `[[lane]]` | `id`, `label`, `kind` = `main` \| `pit` \| `link`, `points` = [ids], `label_x`, `label_y` |
| `[[device]]` | `id`, `kind` = `nsd` \| `sampling` (need `at` = point id) \| `analyzer` \| `zone` (need `x`,`y`,`w`,`h`) \| `turnstile` (see below) \| `gate`, `antenna` (need `at`, see below), `label` |
| `[[actor]]` | `id`, `label`, `sub`, `color` (sequence-diagram lifelines, left to right) |
| `[[carrier]]` | `id`, `tube` (bool), `sample_id`, `route` = [point ids], `start`, `speed` (px/s), `cap_color`, `ghost` (bool, see below), `visible` (bool, default true), `tag` (tag text, default the SampleId) |
| `[[step]]` | `action`, one of `start` / `at` / `after_start` / `after_end`, `delay`, `duration` + action keys (below) |

### Steps

Steps run in file order. Each step's `start` decides when it begins:

- `after` (default): when the previous step ends
- `with`: at the same time as the previous step
- `after_all`: when every previous step has ended

Instead of `start` (only one of these keys per step):

- `at = <seconds>`: absolute time
- `after_start = <seconds>`: that many seconds after the **start** of the previous step
- `after_end = <seconds>`: that many seconds after the **end** of the previous step

`delay` adds seconds on top of any of them. The previous step is the one
before in the file. Example: a message `at = 10` (1.5 s long), then
`after_start = 3` starts at 13 s, while `after_end = 3` would start at 14.5 s.

**With none of these keys** (no `start`, `at`, `after_start`, `after_end`) a step
starts when the previous step ends, i.e. steps run one after the other. The first
step of the file starts at 0 s. To start a step together with the previous one use
`start = "with"` (same as `after_start = 0`). Note that `caption` and `status` last
0 s, so a step right after them starts at the same instant.

| action | keys | default duration |
|---|---|---|
| `caption` | `title`, `text` | stays until the next caption |
| `move` | `carrier`, `to` (a point ahead on its route), `ease` = `linear`\|`in`\|`out`\|`inout`, `speed`, `runaway` (bool: the carrier escapes by itself; red pulsing ring and tag "… · RUN-AWAY" while it moves) | computed from length and speed |
| `message` | `from`, `to` (actor ids), `text`, `params`, `reply` (dashed), `carrier`, `color` (overrides the sender's color) | 1.5 s (arrow travel time) |
| `note` | `actor`, `text` (box on the lifeline) | 1.5 s |
| `pipette` | `device` (a `sampling` or `turnstile` device) | 3 s |
| `highlight` | `device` | 1.5 s |
| `rotate` | `device` (a `turnstile`) | 2 s |
| `show` / `hide` | `carrier` (fades it in / out) | 0.5 s |
| `status` | `device` (an `analyzer` or a `zone`), `state` = `error` (red; ERROR badge on an analyzer) \| `ready` (green) \| `normal` (grey); lasts until the next `status` | 0 s |
| `operator` | `device` (an `analyzer`): a stylized lab operator (white coat, thin blue gloves) walks in from the right edge of the panel, works on the analyzer with a wrench, then walks away (walks take up to 2 s each) | 7 s |
| `needle` | `device` (a `sampling` or `turnstile`), `to` = `down` \| `up`: the needle goes down and stays there until `up` | 0.9 s |
| `wait` | — | `duration` is required |

`{SampleId}` and `{CarrierId}` inside `text` and `params` are replaced with the
carrier's values.

### Turnstile

A wheel with three pockets, 120° apart, at the end of the Pit-Lane. Its three
positions are: entry (at Pit-Lane height), under the analyzer needle (top) and
exit (at Pit-Lane height, on the other side). Each `rotate` moves every pocket
one position forward: entry → needle → exit → entry.

```toml
[[device]]
id = "ts"
kind = "turnstile"
x = 440            # wheel center
y = 505
r = 70             # center → pocket distance (default 70)
entry = "right"    # side of the entry position: right (flow right to left) | left
label = "Turnstile"
```

It generates the points `ts.in`, `ts.needle` and `ts.out`. Use `ts.in` and
`ts.out` in lanes, and write them **consecutively** in carrier routes
(`…, "pit_in", "ts.in", "ts.out", "pit_out", …`). A carrier is moved to `ts.in`
with a `move`, travels on the wheel only by `rotate`, and leaves with a `move`
from the exit (e.g. `to = "merge"`). `pipette` on the turnstile works at the
needle position.

Checked by `--check` (in time order, whatever the order of the steps in the file):
the entry pocket must be free when a carrier arrives, a carrier can leave only
from the exit position, a rotation can't start while a carrier is still in the
exit position, nothing arrives or leaves while the wheel is turning, and the wheel
doesn't turn until a carrier that has left (or is arriving) is completely outside
the wheel's circle.
See `scenes/02_turnstile_three_carriers.toml`.

### Analyzer error and needle

`needle` moves the analyzer needle by hand (unlike `pipette`, which goes down
and back up by itself). While it is down and the analyzer is not in error the
probe shows "Aspirating…"; with `status` `error` it turns red with "Needle down".
`--check` refuses a turnstile rotation, or a carrier leaving a sampling point,
between a `needle down` and the end of the next `needle up`. See scenes 05 and 06.

### Spur

A spur is a U-shaped branch perpendicular to the Main-Lane, built from plain
points and lanes (see `scenes/09_spur_happy_path.toml`): the NSD diverts the
carrier, without stopping, into a short piece of pit-lane that turns 90° into the
up branch; the sampling point is on the up branch with the analyzer on its right;
the carrier crosses at the top and comes back down the other branch to the
Main-Lane. Scenes 09–12 cover the happy path, Clear, analyzer error and run-away.

When the analyzer is beside the sampling point (not above it) the probe comes out
of the analyzer's side and "Aspirating…" / "Needle down" are written inside the
analyzer. If there is no room on the right of the analyzer, the `operator` works
from below it. Carrier tags never cover the analyzer. `nsd` and `sampling`
devices accept `label_x`, `label_y` to move their label.

### Ghost carriers (logical position)

A carrier with `ghost = true` shows where the interface *believes* a carrier is,
while physically it is somewhere else (e.g. Prepare was sent at the NSD, but the
carrier went straight on). It is drawn as a dashed outline under the real
carriers, with a dashed tag; antennas don't read it and gates don't open for it.
Use it together with the real carrier (same `sample_id`, different `id`):
`visible = false` + `show` to make it appear at the NSD, `hide` when it is
cleared. See scenes 03 and 04.

### Gate and antenna

`gate` and `antenna` are placed on a point (`at`) of a horizontal lane; both
work by themselves, without steps:

- **gate**: a barrier across the lane that opens while a carrier drives through
  its point and closes after it. The gate point doesn't need to be in the routes,
  only on their path.
- **antenna**: a reader above the lane. It shows a reading animation while a
  carrier stands still on its point (put that point in the routes, as the stop
  before the gate).

`--check` also refuses a carrier that drives into a turnstile while the analyzer
is pipetting there: it has to wait at the gate until the sampling is over.

When the sequence diagram has more rows than fit, it scrolls up as new rows appear.
If `--check` finds an error, it also prints the timeline up to that point.

Easing tip: `ease = "in"` followed by `ease = "out"` gives one continuous
motion with no stop at the junction. Scene 01 uses this to pass through the
NSD without stopping.
