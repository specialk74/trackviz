# trackviz

Renders automation scenarios to MP4: a top view of the track (carriers, lanes,
NSD, sampling point, analyzer) next to a live sequence diagram (Interface ↔
Analyzer ↔ DMX), with explanatory captions.

Each scene is a text file in `scenes/`. To make a new video you write a new
scene, not new code.

## Requirements

- Rust (stable). The tool uses no external crates.
- `rsvg-convert` (librsvg) and `ffmpeg` on PATH
  - Debian/Ubuntu: `apt install librsvg2-bin ffmpeg`
  - macOS: `brew install librsvg ffmpeg`
- Fonts: Inter (preferred) or DejaVu Sans / DejaVu Sans Mono

## Usage

```sh
cargo build --release
B=./target/release/trackviz

$B scenes/01_prepare_sample_release.toml --check            # validate + print timeline
$B scenes/01_prepare_sample_release.toml --still 17 -o f.png # one frame at t=17 s
$B scenes/01_prepare_sample_release.toml -o out/01.mp4       # full video
```

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
| `[[device]]` | `id`, `kind` = `nsd` \| `sampling` (need `at` = point id) \| `analyzer` \| `zone` (need `x`,`y`,`w`,`h`), `label` |
| `[[actor]]` | `id`, `label`, `sub`, `color` (sequence-diagram lifelines, left to right) |
| `[[carrier]]` | `id`, `tube` (bool), `sample_id`, `route` = [point ids], `start`, `speed` (px/s), `cap_color` |
| `[[step]]` | `action`, `start`, `delay`, `duration` + action keys (below) |

### Steps

Steps run in file order. Each step's `start` decides when it begins:

- `after` (default): when the previous step ends
- `with`: at the same time as the previous step
- `after_all`: when every previous step has ended

`delay` adds seconds on top of that.

| action | keys | default duration |
|---|---|---|
| `caption` | `title`, `text` | stays until the next caption |
| `move` | `carrier`, `to` (a point ahead on its route), `ease` = `linear`\|`in`\|`out`\|`inout`, `speed` | computed from length and speed |
| `message` | `from`, `to` (actor ids), `text`, `params`, `reply` (dashed), `carrier` | 1.5 s (arrow travel time) |
| `note` | `actor`, `text` (box on the lifeline) | 1.5 s |
| `pipette` | `device` (a `sampling` device) | 3 s |
| `highlight` | `device` | 1.5 s |
| `wait` | — | `duration` is required |

`{SampleId}` and `{CarrierId}` inside `text` and `params` are replaced with the
carrier's values.

Easing tip: `ease = "in"` followed by `ease = "out"` gives one continuous
motion with no stop at the junction. Scene 01 uses this to pass through the
NSD without stopping.
