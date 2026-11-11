# trackviz

Renders automation scenarios to MP4: a top view of the track (carriers, lanes,
NSD, sampling point, analyzer) next to a live sequence diagram (Interface ↔
Analyzer ↔ DMX), with explanatory captions.

Each scene is a Mermaid sequence diagram (`.mmd`) in `scenes/`: to make a new video
you write a new scene, not new code. The messages are the diagram; what happens on the
track goes in `%%` comment lines, which Mermaid doesn't show.

## Requirements

- Rust (stable).
- `ffmpeg` on PATH, only for the MP4 (`--still`, `--check` and `--layout` don't need
  it): `apt install ffmpeg`, `brew install ffmpeg` or `winget install Gyan.FFmpeg`.
- Nothing else: the fonts are built into the executable (see `fonts/README.md` for
  offline builds).

## Commands

```sh
cargo build --release
B=./target/release/trackviz

$B scenes/a.mmd                  # the video: out/a.mp4 (and out/a_layout.png)
$B scenes/a.mmd -o other.mp4     # the video, chosen file name
$B scenes/a.mmd --check          # validate and print the timeline, no video
$B scenes/a.mmd --layout         # only out/a_layout.png: names of points and devices
$B scenes/a.mmd --sheet          # out/a_sheet.png: one frame per caption, at its end
$B scenes/a.mmd --still 17 -o f.png   # one frame at 17 s
$B scenes/a.mmd --preview        # half size, 15 fps: out/a_preview.mp4
$B scenes/a.mmd --from 15 --to 25     # only that part (also out/a_preview.mp4)
$B scenes/a.mmd --preview --watch     # again every time the scene is saved (Ctrl-C stops)
$B "scenes/*.mmd"                # several scenes, in parallel
$B new my_scene --geometry spur2 --carriers 3   # a new scene: scenes/my_scene.mmd
$B index                         # out/index.html: the page that lists the videos
$B sort scenes/a.mmd             # the steps in time order, same video (--dry-run: only say it)
cargo test --release             # the tests, all the scenes included
```

- `--sheet` is for checking a scene without watching the video: a frame for every
  caption, taken just before the next caption appears, two per row.
- `index` writes one page for handing the videos over: for every scene its subtitle,
  length, the video with a cover frame and its captions as chapters (a click plays
  the video from there). Default all of `scenes/`; give scenes or `-o` to change it.
  The page needs no other file: hand over `out/index.html` with the `.mp4` files next
  to it. It warns about videos that are not there yet.
- `--watch` takes one scene and also works with `--check` and `--still`.
- With several scenes a failure doesn't stop the others; they are listed at the end and
  the exit code is 1. `-o` and `--still` take one scene.
- `sort` rewrites a scene with its steps in the order in which they start in the
  video, so that the Mermaid diagram (which follows the file) shows the messages in the
  same order as the video. Each step moves with the directives above it (`on`, `after`,
  `id`, `color`, `for`, comments); where a moved step was tied to the line before it
  (no timing, `after <seconds>`, `after <seconds> pipette ...`), it is tied to the
  same step with an `id` (`%% id pipette-antenna4` + `%% after 0 pipette-antenna4`).
  The sorted scene is built first and must give the same timeline: otherwise the file
  is left as it is. `--dry-run` only says what it would do.
- `new` writes a scene that is valid as it is, for one geometry (`sampling`,
  `sampling_queue`, `sampling_queue2`, `turnstile`, `turnstile_queue`, `spur`, `spur2`;
  default `sampling`): carrier C1 goes the whole way, the others are declared. It never
  overwrites a file.

`--check` prints every step with its start and end time, and warnings for what is
valid but looks wrong (they don't stop anything), on the line of the step: carriers
overlapping, captions too long for the band, captions shown less than 2.5 s or out of
order, messages written in a different order from the one in which they are sent
(`sort` fixes these two). On an error it prints the timeline up to that point.

**VS Code.** With the project open: type `tv-` for a snippet of every directive and
message (`tv-on`, `tv-pipette`, `tv-msg-prepare`, `tv-scene`, ...); Ctrl+Shift+B runs
`--check` on the open scene and puts errors and warnings on their lines. More tasks
(preview, video, layout, contact sheet, sort in time order, check all) in Terminal >
Run Task.

## Writing a scene

```
%% geometry spur
%% subtitle Spur — one carrier, happy path
sequenceDiagram
    participant IF as Interface
    participant AN as Analyzer
    participant DMX as DMX

    %% carrier id=C1 sid=SID-000123

    %% caption 1 · Start | The carrier travels along the Main-Lane.
    %% at 2.5
    %% move C1 to antenna2
    %% on antenna1
    IF->>AN: Prepare SampleId=SID-000123
    %% after 0.1
    AN-->>IF: OK SampleId=SID-000123
    %% on antenna2
    IF->>AN: Sample Acquisition SampleId=SID-000123
    %% after 0.1
    %% pipette antenna2 duration=4 sid=SID-000123
    %% after 0.3 pipette antenna2
    AN->>IF: Sample Complete SampleId=SID-000123
    %% after 0.1
    %% pass antenna2 C1
```

`geometry` loads the track. Every scene also reads `common/video.mmd` (title, size,
belt speed), `common/actors.mmd` and `common/messages.mmd` (`common/` is looked for next
to the scene, then in `scenes/`). The participants are matched with the actors by id or
label. `SampleId=` in a message must be the `sid=` of a carrier, and it makes that
carrier the carrier of the message. As in Mermaid, `<br/>` in a message or a note
starts a new line (`IF->>AN: EAC^U07<br/>SAC-3: SID-000123`): the label is drawn on
more lines, above its arrow. `alt`, `loop`, `opt` and `par` are refused: write
one diagram for each case.

### When a step starts

Every step (message, note, move, caption, ...) starts when the one before it ends,
unless a directive right before it says otherwise:

| Directive | The next step starts |
|---|---|
| `on <antenna>` | when its carrier is over that antenna: the carrier of the `SampleId` in the message, or the one it moves or shows |
| `on <antenna> <carrier>` | when that carrier is over the antenna (required for captions, highlights, pipettes, notes) |
| `after <seconds>` | that many seconds after the step before has ended (a message ends when it arrives, a move when the carrier arrives, a `pass` or `divert` when the carrier sets off) |
| `after <seconds> <id>` | that many seconds after the line marked `id <id>` has ended |
| `after <seconds> <antenna> <carrier>` | that many seconds after the carrier is over the antenna |
| `after <seconds> pipette\|needle <antenna>`, `after <seconds> rotate <turnstile>` | that many seconds after that action ends |
| `at <seconds>` | at that second of the video |

Only one of them per step. To wait for a line that is not the one right before, give
that line a name with `%% id <name>` above it, then write `%% after <seconds> <name>`:

```
    %% id ack1
    AN-->>IF: OK SampleId=SID-000123
    ...
    %% after 0.1 ack1
    %% pass antenna2 C1
```

The id must be written before the `after` that uses it, and each id is used once per
file. A step tied to an antenna or to another line moves with it when a carrier has to
wait, so the scene works at any belt speed.

The antennas of each geometry, numbered in the direction of the carriers:

| Geometry | Antennas |
|---|---|
| `sampling`, `spur` | 1 NSD, 2 sampling point, 3 end of the pit-lane (Release) |
| `turnstile` | 1 NSD, 2 in front of the gate, 3 under the needle (for `pipette` and `needle`: the wheel brings the carriers there, so `on`/`after` can't wait for it; start from `rotate`), 4 end of the pit-lane (Release) |
| `spur2` | 1 NSD, 2 on the spur before Divert 1 (with a gate: it only lets the carrier go on with `pass antenna2 <carrier>`, it can't divert), 3 Divert 1, 4 sampling point 1, 5 Divert 2, 6 sampling point 2, 7 end of the spur (Release) |

### All the directives

Each one is written `%% <directive> ...` on a line of its own. In the forms below the
words in `<...>` are **required**; the `key=value` options and the plain words in the
last column are **optional**, in any order, and the column says what happens without
them. A missing required word is an error that shows the right form; so is an unknown
directive (with a suggestion when it looks like a known one).

The directives in **Steps** become something that happens in the video. A message
(`IF->>AN: ...`) and a note (`Note over AN: ...`) of the diagram are steps too: 1.5 s
each.

**Scene and carriers.** These set up the scene and take no time.

| Directive | What it does | Optional |
|---|---|---|
| `geometry <name>` | Loads the track from `common/<name>.mmd`: points, lanes, devices, antennas, gates. One of `sampling`, `sampling_queue`, `sampling_queue2`, `turnstile`, `turnstile_queue`, `spur`, `spur2`. Write it before the carriers | — |
| `title <text>` | The title at the top of the video | without it: the title in `common/video.mmd` |
| `subtitle <text>` | The line under the title | without it: no subtitle |
| `carrier id=<id>` | Declares a carrier. Declare all of them before the steps; the order counts when two start on the same spot (the first one goes first) | `sid=<sample id>`: the sample in its tube, used by `SampleId=` in the messages; without it the carrier is empty (no tube, tag `<id> (empty)`).<br>`start=<antenna>\|<point>`: where it is at the beginning; default the Main-Lane for the first carrier (`main_in`), off the panel for the others (`spawn`). Its way is not written: it comes from its `move`, `pass` and `divert` (see The way of a carrier).<br>`color=#RRGGBB`: color of its cap; default purple `#7C3AED`.<br>`speed=<px/s>`: its own speed; default the belt's.<br>`tag=<text>`: the text of its label, `_` for spaces; default the sample id (`<id> (empty)` without a tube).<br>`ghost`: drawn dashed, where the interface *believes* it is (see Ghost carriers).<br>`hidden`: not shown until a `show` |
| `include <file>` | Puts here the `%%` lines of another file (path relative to this one) | — |
| `@<section> key=value ...` | A row of a data table (see Scene format) | — |
| `# <text>` | A comment, ignored | — |

**Before a step.** These change the next step only.

| Directive | What it does | Optional |
|---|---|---|
| `on <antenna>` | The next step starts when its carrier is over the antenna | `<carrier>` after the antenna: which carrier. Without it, the carrier of the step (the `SampleId` of the message, the one it moves or shows); for captions, highlights, pipettes and notes it is required |
| `after <seconds>` | The next step starts that many seconds after the step before ends | what to wait for: `<id>` (the end of the line marked with that id), `<antenna> <carrier>` (the carrier over the antenna), `pipette\|needle <antenna>` or `rotate <turnstile>` (the end of that action) |
| `at <seconds>` | The next step starts at that second of the video | — |
| `id <name>` | Names the next line (message, note, move, pipette, ...), so that a later `after <seconds> <name>` can start from its end. Letters, digits, `-`, `_`; one per line, never used twice in a file | — |
| `color #RRGGBB` | Colors the next message (the purple Prepare is `#9333EA`) | without it: the color of the named message or of the sender |
| `for <carrier>` | The carrier of the next message, when its text has no `SampleId=` (e.g. `OK`): `on` and the pit-lane capacity use it | without it: the message has no carrier |
| `duration <seconds>` | How long the next step lasts; for a message, how long its arrow takes to arrive | without it: the step's default (a message 1.5 s, or `%% @video message=<s>` for the whole scene) |

Only one of `on`, `after`, `at` per step.

**Steps.**

| Directive | What it does | Optional |
|---|---|---|
| `move <carrier> to <antenna>\|<point>` | An antenna stands for its point (`move C1 to antenna2`); other places by the name of the point (`gone`, `iface_out`: see `--layout`). The carrier sets off towards the point, by the shortest way along the lanes from where it is (see The way of a carrier). It rides the belt at constant speed, queues behind other carriers and stops at the point. The step ends when it arrives | `duration=<s>`: the time of the trip; without it, from length and speed.<br>`speed=<px/s>`: only for this move; default the carrier's.<br>`ease=linear\|in\|out\|inout`: default `linear` (the belt starts and stops at once).<br>`runaway`: the carrier escapes by itself (red pulsing ring, tag "… · RUN-AWAY") |
| `pass <antenna> <carrier>` | The gate on the antenna opens and the carrier, stopped there, goes on straight: along the lanes to the next stop (an antenna with a gate, a turnstile entry) or to the end of the track. The step ends when the carrier sets off: the next step starts then, not when it arrives (to wait for the arrival: `on <antenna> <carrier>`). Only antennas, not the names of devices (`pass sp C1` is an error that names `pass antenna2`). The carrier must have been sent to the antenna before (`move C1 to antenna2`, or a `pass`/`divert` that stops there), and be the one stopped there at that moment. If it went straight on because the pit-lane was full, its `pass` is skipped | — |
| `divert <antenna> <carrier>` | Like `pass`, but the carrier turns into the lane that starts on the antenna (its pit-lane), then goes on to the next stop. Only on the antenna of a divert (`spur2`: `antenna3` Divert 1, `antenna5` Divert 2); elsewhere it is an error | — |
| `pipette <antenna>` | Sampling, on the sampling point of the antenna (`pipette antenna2`; `spur2`: `antenna4` needle 1, `antenna6` needle 2; turnstile: `antenna3`, under the needle). The name of the device is refused (`pipette sp` names `pipette antenna2`). The analyzer's arm goes to the tube there, the needle goes down, aspirates and comes back up (20% down, 60% aspiration, 20% up). It also adds the note "Pipetting Analyzer" on the analyzer's lifeline, for the same time. It only draws the sampling: it sends no message and does not check that a carrier is under the needle (in scene 08 it samples an empty pocket). The device must be a sampling point or a turnstile, and the scene must have an analyzer. Lasts 3 s | `duration=<s>`: how long it lasts.<br>`sid=<sample id>`: the sample the analyzer believes it is aspirating (the one of its Sample Acquisition), added to the note: "Pipetting Analyzer - SampleId=SID-000123". Without it the note has no SampleId; and if that carrier goes straight on because the pit-lane is full, this pipette is not skipped.<br>A `Note over AN: ...` right after it replaces the text of the note |
| `rotate <device>` | The turnstile turns by one position: entry → needle → exit. It waits by itself until the wheel is free (see Turnstile). Lasts 2 s | `duration=<s>` |
| `highlight <device>` | The device lights up. Not needed when an antenna reads a carrier: the NSD, a divert or an antenna on its own light up by themselves. Lasts 1.5 s | `duration=<s>` |
| `needle <antenna> down\|up` | On the sampling point of the antenna, like `pipette`. Moves the needle by hand: down it stays there until `up`. Meanwhile the wheel can't turn and the carrier can't leave. The device must be a sampling point or a turnstile. Lasts 0.9 s | `duration=<s>` |
| `status <device> error\|ready\|normal` | The color of an analyzer, a zone or a sampling point (one needle): red, green, grey, until the next `status`. Takes no time | — |
| `operator <device>` | An operator walks in, works on the analyzer and walks away. Lasts 7 s | `duration=<s>`, at least 2 |
| `show <carrier>` / `hide <carrier>` | The carrier fades in / out. Lasts 0.5 s | `duration=<s>` |
| `wait <seconds>` | Nothing happens for that time | — |
| `caption <title> \| <text>` | A caption in the band, until the next caption. Takes no time | — |


### The track

**Belt and carriers.** The carriers ride a belt that never stops: `belt=` (cm/s) in
`common/video.mmd`, default 17.5 (a carrier, 4 cm wide, is 44 px on screen); a scene
can set its own with `%% @video belt=10`. Every move is at that constant speed.
Two carriers never take the same space and they only go forward: a carrier that
reaches another one stops in contact and moves on when that one makes room. `--check`
shows it: `move C2 -> sampling  (waits 12.0s behind C1)`.

- Where two lanes join, the carrier nearer to the junction goes first (on a tie, the
  one declared first); carriers on the same spot go in the order they are declared.
- Hidden carriers and carriers in a turnstile pocket don't take space. A ghost can't
  drive through a carrier in front of it (it stops behind it, real or ghost), but the
  carriers behind it drive through it; it doesn't wait for its own real carrier (same
  `sid`). Outside the panel carriers don't stop.

**Antennas and gates.** The antennas lie on the lanes, under the carriers. Wherever a
carrier must stop on an antenna (sampling points, antenna 2 and the diverts of `spur2`, in front of
the turnstile) a gate holds it there, until `pass` (or `divert`); at the NSD the carrier is read on
the fly. The gate opens and closes in 0.22 s. The last antenna is not drawn: it only
triggers the Release. When an antenna reads a carrier the NSD, a divert or the antenna
itself lights up by itself, at least 1 s.

**The way of a carrier.** It is not written: it comes from its steps, in the order of
the file. The carrier starts on its `start`; `move` takes the shortest way along the
lanes (in their direction) to the antenna or point, so `move C1 to antenna2` goes into
the pit-lane and `move C2 to gone` straight on along the Main-Lane; `pass` goes on
straight to the next stop, `divert` turns into the lane that starts on the antenna.

```
%% carrier id=C1 sid=SID-000123          (starts on the Main-Lane, main_in)
%% carrier id=C2 sid=SID-000124          (starts off the panel, spawn)
%% move C1 to antenna3                   (into the spur, up to Divert 1)
%% divert antenna3 C1                    (into the pit-lane of needle 1, up to antenna4)
%% move C2 to gone                       (straight on the Main-Lane)
```

The layout image (`--layout`) shows the name of every point and device.

**Pit-lane capacity.** A lane can hold at most `capacity=` carriers (in the geometry;
the sampling pit-lane holds 10). A carrier counts from the NSD until it has passed the
first antenna or gate. One that would find the lane full goes straight on along the
Main-Lane, and its steps after the diversion are skipped by themselves (moves,
`pass`/`divert`, messages, the pipettes of its `sid`, and the steps that depend on them); `--check` says
so at the top of the timeline.

**Turnstile.** A wheel with three pockets: entry, under the needle, exit. A carrier
enters with a `pass` from the gate (or a `move` to `ts.in`), travels only by `rotate`,
and leaves with a `move` from the exit. A `rotate` waits by itself until the wheel is
free (`waits 0.5s for the wheel to be clear` in `--check`). `--check` refuses a
carrier entering a busy entry pocket or entering while the analyzer is pipetting
there, a carrier leaving from another position, and a rotation with a carrier still in
the exit or with the needle down.

**Ghost carriers.** `%% carrier ... ghost` is drawn dashed: where the interface
*believes* a carrier is, while the real one (same `sid`, another id) is somewhere else.
Antennas don't read it and gates don't open for it; it queues behind the carriers in
front, but doesn't hold up anyone. It moves with `move`, `pass` and `divert` like the
others (written with its id: `divert antenna3 C2L`). Usually `hidden`, then `show` when
it should appear and `hide` when it is cleared (scenes 03 and 04).

**Analyzer error and needle.** `status AN error|ready|normal` colors the analyzer
(also a zone, or one needle of `spur2` with its sampling point). `needle <antenna> down`
keeps the needle in the tube until `needle ... up` (scenes 05, 06).

## More

- `doc/FORMAT.md`: the `%% @<section>` tables behind the directives and the
  geometries, and how each device works: needed to change or make a geometry.
- `doc/PROJECT.md`: the design and the scenarios, in Italian.
