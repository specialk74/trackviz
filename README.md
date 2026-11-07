# trackviz

Renders automation scenarios to MP4: a top view of the track (carriers, lanes,
NSD, sampling point, analyzer) next to a live sequence diagram (Interface ↔
Analyzer ↔ DMX), with explanatory captions.

Each scene is a Mermaid sequence diagram (`.mmd`) in `scenes/`. To make a new video
you write a new scene, not new code.

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
cargo test --release      # unit tests (scene reading, timeline rules, rendering, all scenes)
B=./target/release/trackviz

$B scenes/01_prepare_sample_release.mmd --check            # validate + print timeline
$B scenes/01_prepare_sample_release.mmd --still 17 -o f.png # one frame at t=17 s
$B scenes/01_prepare_sample_release.mmd                    # full video → out/01_prepare_sample_release.mp4
$B scenes/01_prepare_sample_release.mmd -o other.mp4        # full video, chosen file name
```

Without `-o` the video goes to `out/<scene file name>.mp4` (`out/` is created if missing).

### Mermaid

A scene is a Mermaid sequence diagram with `%%` directives: every command takes it,
also with wildcards, and the video is `out/<name>.mp4`.

```sh
$B flow.mmd                  # the video
$B flow.mmd --preview --watch
$B flow.mmd --layout         # only out/flow_layout.png: where the points and devices are
$B flow.mmd --check
$B "scenes/*.mmd"            # all the diagrams, in parallel
```

A full video also writes `out/<scene name>_layout.png`: the track without carriers,
with the name of every point and device (hidden antennas as a dashed circle) and the
list of the devices, to see where they are when writing a scene. `--layout` writes
only this image.

Mermaid only describes the messages, so the physical side goes in `%%` comments, which
don't appear in the diagram (see `scenes/17_mermaid_spur_happy_path.mmd`):

```
%% include common/spur_one_carrier.mmd
sequenceDiagram
    %% on antenna1
    IF->>AN: Prepare SampleId=SID-000123
    AN-->>IF: OK SampleId=SID-000123
    %% pipette sp duration=4
```

`common/` is looked for next to the diagram, then in `scenes/`: it holds the
geometries, actors, messages and video settings that scenes share (see Scene format).

`%% include <file>` puts the directives of another file in that place (path relative
to the including file, nested includes allowed, circular ones refused). An included
file can only hold `%%` lines: a message there would be missing from the diagram that
Mermaid shows, so messages stay in the main file.

**When a step starts.** Every step (message, note, move, caption, ...) starts when the
one before it ends, unless a directive right before it says otherwise:

| Directive | The next step starts |
|---|---|
| `on <antenna>` | when its carrier is over that antenna: the carrier of the `SampleId` in the message, or the one it moves or shows |
| `on <antenna> <carrier>` | when that carrier is over the antenna (for captions, highlights, pipettes, notes, ...) |
| `after <seconds>` | that many seconds after the step before has ended (a message ends when it arrives) |
| `after <seconds> <message> [<carrier>]` | that many seconds after the latest message with that text (for that carrier) has arrived |
| `after <seconds> <antenna> <carrier>` | that many seconds after the carrier is over the antenna |
| `after <seconds> pipette\|rotate\|needle <device>` | that many seconds after that action on the device ends |
| `at <seconds>` | at that second of the video |

Every geometry has its antennas, numbered in the direction of the carriers:

| Geometry | Antennas |
|---|---|
| `sampling`, `spur` | 1 NSD, 2 sampling point, 3 end of the pit-lane (Release) |
| `turnstile` | 1 NSD, 2 in front of the gate, 3 end of the pit-lane (Release) |
| `spur2` | 1 NSD, 2 Divert 1, 3 sampling point 1, 4 Divert 2, 5 sampling point 2, 6 end of the spur (Release) |

The antennas lie on the lanes, under the carriers, without labels. The belt under the
carriers never stops, so wherever a carrier must stop on an antenna a gate holds it
there (sampling points, the divert points of `spur2`; in front of the turnstile the
gate was already there). At the NSD the carrier is read and diverted on the fly. The
last antenna is not drawn: it is at the end of the pit-lane, before the carrier is back
on the Main-Lane, and only triggers the Release.

The message keeps the text written in the diagram exactly as it is:

```
%% on antenna1
IF->>AN: Add Tube SampleId=SID-000123
%% after 0.1
AN-->>IF: OK SampleId=SID-000123
%% color #9333EA
%% on antenna2
IF->>AN: Add Tube SampleId=SID-000123
```

`color #RRGGBB` colors the next message (the purple Prepare is `#9333EA`). Only one of
`on`, `after`, `at` per step. `after <seconds> <message>` refers to the latest message
with that text written before it, so keep the diagram in time order. With the carrier
physics (below) a step tied to an antenna or to a message moves with it when a carrier
has to wait. `when`, `with`, `delay`, `after_start` and `after_end` no longer exist.

| Directive | Meaning |
|---|---|
| `geometry <name>` | reads `common/<name>.mmd`: `sampling`, `turnstile`, `spur`, `spur2`, ... |
| `title`, `subtitle` | the video texts |
| `for <carrier>` | the carrier of the next message, when its text doesn't say it (e.g. `OK`) |
| `duration <seconds>` | duration of the next message or note |
| `carrier id=<id> [sid=<sample id>]` | without `sid=` the carrier is empty (no tube, its tag reads `<id> (empty)`); options: `needle=1\|2\|both` (spur2), `route=a,b,c`, `color=`, `speed=`, `tag=`, `ghost`, `hidden` |
| `move <carrier> to <point>` | options: `ease=`, `speed=`, `duration=`, `runaway` |
| `pass <device>` | the carrier stopped on the device (`sp`, `div1`, `antenna2`, ...) sets off and rides the belt to the next stop of its route (an antenna with a gate, a turnstile entry) or to its end. No carrier stopped there is an error |
| `pipette`, `rotate`, `highlight`, `operator <device>` | option: `duration=`; `pipette` also `sid=` (the sample it aspirates, shown in its note) |
| `needle <device> down\|up`, `status <device> <state>` | as the steps |
| `show`, `hide <carrier>`, `wait <seconds>` | as the steps; `show` and `hide` take `duration=` |
| `caption <title> \| <text>` | a caption |
| `on`, `after`, `at`, `color` | see above |
| `include <file>` | the directives of another file |
| `@<section> key=value ...` | a table of data (see Scene format) |
| `# text` | a plain comment |

An unknown directive is an error, so that typos don't get lost, and errors name the
line of the diagram. Participants are
matched with the shared actors by id or label. Messages become named messages when
they match `common/messages.mmd` (with the same color). `SampleId=` in a message must be the `sid=` of a declared carrier. `alt`, `loop`, `opt` and `par` are
refused: a scene is one path, so write one diagram for each case.

To look at a scene quickly while writing it:

```sh
$B scenes/a.mmd --preview                # half size, 15 fps
$B scenes/a.mmd --from 15 --to 25        # only that part, full quality
$B scenes/a.mmd --preview --from 15 --to 25 --watch
```

These write `out/<scene file name>_preview.mp4`, so the full video is kept. `--watch`
repeats the command every time the scene or a file it includes is saved; if the file
has an error it prints it and keeps watching. Stop it with Ctrl-C. It takes one scene
and also works with `--check` and `--still`.

Several scenes can be given at once, also with the wildcards `*` and `?` in the file
name; they are rendered in parallel:

```sh
$B "scenes/*.mmd"            # all the scenes
$B "scenes/1?_spur*.mmd"     # some of them
$B a.mmd b.mmd --check      # validate several scenes
```

Use the quotes to let trackviz expand the wildcards (needed on Windows); without them
the shell does it, with the same result. The `-j` threads are shared among the scenes:
`min(scenes, jobs)` run at the same time, with `jobs / scenes` threads each (at least 1).
A scene that fails doesn't stop the others: all the failures are listed at the end and
the exit code is 1. `-o` and `--still` take a single scene.

`--check` prints every step with its start and end time. Use it as the event
log when tuning timings. It also prints warnings, as rendering does, for what is
valid but looks wrong; they don't stop anything and the exit code stays 0:

- two physical carriers overlapping on the track (with the physics this happens only
  when they start on the same spot inside the panel)
- captions too long for the band, which get cut
- captions shown less than 2.5 s before the next one, or written in a different order
  from the one in which they appear
- messages written in a different order from the one in which they are sent (the
  video follows time, the Mermaid diagram follows the file)
- a Query sent before the OK to its Prepare or the reply to the previous Query has
  arrived

### Routes

A route lists where the carrier goes: points and lane ids, in order. A lane id stands
for all its points; between two places the program fills in the way along the lanes
(in their direction, through a turnstile from `ts.in` to `ts.out`). Two points that no
lane joins are joined by a straight line.

```
%% carrier id=C1 sid=SID-000123 route=main_in,pit,main_out
     -> main_in, nsd, pit_in, sampling, pit_out, merge, main_out
%% carrier id=C2 sid=SID-000124 route=spawn,gone        (straight on the Main-Lane)
```

| Geometry | Lanes |
|---|---|
| `sampling`, `sampling_queue`, `sampling_queue2` | `main`, `pit` |
| `turnstile`, `turnstile_queue` | `main`, `pit_entry`, `pit_exit` |
| `spur` | `main`, `spur` |
| `spur2` | `main`, `spur`, `pit1` (needle 1), `pit2` (needle 2) |

The layout image shows every point: a route can also name them one by one.

### Carrier physics

Carriers ride a belt that runs at 17.5 cm/s, 192.5 px/s on screen (a carrier, 4 cm
wide, is drawn 44 px wide): every move is at that constant speed, starting and stopping
at once. `speed=` and `ease=` are still accepted, but the scenes don't use them.

Two carriers never take the same space, and they only go forward. `move C2 to
sampling` means "C2 sets off towards the sampling point": if the way is taken, C2
stops in contact with the carrier in front and moves on as soon as that one makes
room, so it arrives when it can. Everything that waits for that arrival (`on
antennaN`, `when C2 at ...`) moves with it. `--check` shows the waits:

```
     0.00s -   20.35s  move      C2 -> sampling  (waits 12.0s behind C1)
    30.35s -   43.83s  move      C2 -> iface_out  (stops behind C1 and never gets there)
```

- Where two lanes join, the carrier nearer to the junction goes first (on a tie, the
  one declared first); the other one stays clear of its way and then follows it.
- Carriers that start on the same spot (the spawn point) go in the order they are
  declared.
- A `move` asked while the carrier is still moving starts when it stops.
- `pass sp` releases the carrier held at the gate of `sp`: the scenes use it instead of
  a `move` whenever a carrier sets off from a gate.
- A lane can have a `capacity` (in the geometry: `%% @lane id=pit ... capacity=3`). A
  carrier counts from when it leaves the Main-Lane until it is back on it; one that
  would find the lane full goes straight on along the Main-Lane to the end of its
  route. Its steps after the diversion are skipped: its moves past that point, its
  messages (Prepare included), the pipettes of its `sid`, the steps that start when
  it reaches the diversion or a point it no longer passes, and those that start from
  them. `--check` says so at the top of the timeline. Ghosts don't count.
- Ghosts, hidden carriers and carriers in a turnstile pocket don't take space;
  outside the panel carriers don't stop.

## Scene format

A scene is a Mermaid sequence diagram (`.mmd`). Besides the directives above, a
`%% @<section> key=value ...` line describes one piece of data, a table: a point, a
lane, a device, an actor, a named message, a carrier, a step, or the video settings.
The directives produce the same tables, so a scene can mix the two.

```
%% @point id=nsd x=800 y=760
%% @lane id=main kind=main label="Main-Lane" points=spawn,main_in,nsd,merge label_x=470 label_y=820
%% @device id=analyzer kind=analyzer x=730 y=330 w=200 h=220 label=Analyzer
%% @video title="Analyzer interface" width=1920 height=1080 fps=30 tail=3.5
```

Values are `true`/`false`, numbers, comma-separated lists (`points=a,b,c`), quoted
text when it has spaces (`label="Main-Lane"`, with `\"` and `\\` inside), or a single
word. A key may appear once per line. `@video` lines add keys to the one video table,
the last one winning; every other section is a list, in file order.

Every scene reads `common/video.mmd`, `common/actors.mmd` and `common/messages.mmd`
first; `%% geometry <name>` reads `common/<name>.mmd`. Shared files hold only `%%`
lines. An id defined twice is an error, and errors name the file and line of the table.
A geometry that differs needs its own common file.

| Common file | Contents |
|---|---|
| `video.mmd` | title, size, fps, tail, clock |
| `actors.mmd` | Interface, Analyzer, DMX |
| `messages.mmd` | the named messages (below) |
| `sampling.mmd` | Pit-Lane with a simple sampling point |
| `turnstile.mmd` | Pit-Lane with turnstile, gate and antennas |
| `sampling_queue.mmd`, `sampling_queue2.mmd` | the same with one or two queue positions |
| `turnstile_queue.mmd` | turnstile with a queue position behind antenna 2 |
| `spur.mmd` | spur with one sampling point |
| `spur2.mmd` | spur with two sampling points |
| `spur_one_carrier.mmd` | directives: one carrier arriving at the sampling point of a spur |

All coordinates are in a fixed 1920×1080 space. The track panel covers
x 30–950 and y 120–885. The video `width` and `height` only scale the output.

| Section | Keys |
|---|---|
| `@video` | `title`, `subtitle`, `width`, `height`, `fps`, `tail` (seconds held at the end), `show_clock`, `show_times` |
| `@point` | `id`, `x`, `y` |
| `@lane` | `id`, `label`, `kind` = `main` \| `pit` \| `link`, `points` = list of ids, `label_x`, `label_y`, `capacity` (most carriers in it at once, see Carrier physics) |
| `@device` | `id`, `kind` = `nsd` \| `sampling` \| `divert` (need `at` = point id) \| `analyzer` \| `zone` (need `x`,`y`,`w`,`h`) \| `turnstile` (see below) \| `gate`, `antenna` (need `at`, see below), `label` |
| `@actor` | `id`, `label`, `sub`, `color` (sequence-diagram lifelines, left to right), `analyzer` (this actor gets the "Pipetting" note) |
| `@message` | `id`, `from`, `to`, `text`, `params`, `reply`, `color` (named message, see below) |
| `@carrier` | `id`, `tube`, `sample_id`, `route` = list of points and lane ids (see Routes), `start`, `speed` (px/s), `cap_color`, `ghost` (see below), `visible` (default true), `tag` (tag text, default the SampleId) |
| `@step` | `action`, one of `start` / `at` / `after_start` / `after_end` / `when`, `delay`, `duration` + action keys (below) |

### Steps

These are the keys of a `@step` table, what the directives produce: `on antenna1` gives
`when="C1 at nsd"`, `after 0.2 pipette sp` gives `when="pipette sp done" delay=0.2`,
`after 1` gives `after_end=1`. Written by hand in `@step` lines they still work. Rows of
the sequence diagram always follow time, whatever the order in the file.


- `after` (default): when the previous step ends
- `with`: at the same time as the previous step
- `after_all`: when every previous step has ended

Instead of `start` (only one of these keys per step):

- `at=<seconds>`: absolute time
- `after_start=<seconds>`: that many seconds after the **start** of the previous step
- `after_end=<seconds>`: that many seconds after the **end** of the previous step
- `when`: when something happens, so the step follows it if the timing changes:
  - `when="C1 at merge"`: the carrier reaches the point, also in the middle of a move
  - `when="query C1 sent"` / `"query C1 arrived"`: a message starts / ends (named, or by `event`);
    the carrier is optional (`"status_error sent"`)
  - `when="pipette sp done"`: a `pipette`, `rotate` or `needle` on that device ends

  It refers to the latest matching step **before** it in the file, so write steps in
  time order. Add `delay` for a wait after the event. Several causes joined by `and`
  start the step at the last one.

`delay` adds seconds on top of any of them. The previous step is the one
before in the file. Example: a message `at=10` (1.5 s long), then
`after_start=3` starts at 13 s, while `after_end=3` would start at 14.5 s.

**With none of these keys** (no `start`, `at`, `after_start`, `after_end`, `when`) a step
starts when the previous step ends, i.e. steps run one after the other. The first
step of the file starts at 0 s. To start a step together with the previous one use
`start=with` (same as `after_start=0`). Note that `caption` and `status` last
0 s, so a step right after them starts at the same instant.

| action | keys | default duration |
|---|---|---|
| `caption` | `title`, `text` | stays until the next caption |
| `move` | `carrier`, `to` (a point ahead on its route), `ease` = `linear`\|`in`\|`out`\|`inout`, `speed`, `runaway` (bool: the carrier escapes by itself; red pulsing ring and tag "… · RUN-AWAY" while it moves) | computed from length and speed |
| `message` | `type` (a named message), `carrier`, `needle`; or, without `type`: `from`, `to` (actor ids), `text`, `params`, `reply` (dashed), `carrier`, `needle`, `color` (overrides the sender's color) | 1.5 s (arrow travel time) |
| `note` | `actor`, `text` (box on the lifeline) | 1.5 s |
| `pipette` | `device` (a `sampling` or `turnstile` device), `sample_id`, `note`. Also adds a note on the `analyzer` actor, with the same timing: "Pipetting Analyzer - SampleId=SID-000123" ("· Needle n" when there are several sampling points). The SampleId is the one written on the directive, `%% pipette sp duration=4.5 sid=SID-000123`; without it the note has none. A `Note over AN: ...` written right after the `pipette` in the diagram replaces this text | 3 s |
| `highlight` | `device`. Not needed when an antenna reads a carrier: the NSD, a divert or an antenna on its own light up by themselves while the carrier is on the antenna, at least 1 s; ghosts and hidden carriers are not read, sampling points don't light up | 1.5 s |
| `rotate` | `device` (a `turnstile`) | 2 s |
| `show` / `hide` | `carrier` (fades it in / out) | 0.5 s |
| `status` | `device` (an `analyzer`, a `zone`, or a `sampling` device for one needle), `state` = `error` (red; ERROR badge on an analyzer) \| `ready` (green) \| `normal` (grey); lasts until the next `status` | 0 s |
| `operator` | `device` (an `analyzer`): a stylized lab operator (white coat, thin blue gloves) walks in from the right edge of the panel, works on the analyzer with one hand, then walks away (walks take up to 2 s each) | 7 s |
| `needle` | `device` (a `sampling` or `turnstile`), `to` = `down` \| `up`: the needle goes down and stays there until `up` | 0.9 s |
| `wait` | — | `duration` is required |

`{SampleId}` and `{CarrierId}` inside `text` and `params` are replaced with the
carrier's values, `{Needle}` with the `needle` key of the step (required then).

### Named messages

Messages are defined once in `scenes/common/messages.mmd`. A message written in a
diagram that matches one of them becomes that message, so a change there applies to
every scene:

```
%% @message id=sample_acquisition_needle from=IF to=AN text="Sample Acquisition" params="SampleId={SampleId} Needle={Needle}"

IF->>AN: Sample Acquisition SampleId=SID-000123 Needle=1
```

As a table, the message is `%% @step action=message type=sample_acquisition_needle
carrier=C1 needle=1`: a step with `type` can't redefine the message, only `carrier`
and `needle`, plus the timing keys, are allowed. A message that matches no type keeps its own text.

### Turnstile

A wheel with three pockets, 120° apart, at the end of the Pit-Lane. Its three
positions are: entry (at Pit-Lane height), under the analyzer needle (top) and
exit (at Pit-Lane height, on the other side). Each `rotate` moves every pocket
one position forward: entry → needle → exit → entry.

```
%% @device id=ts kind=turnstile x=440 y=505 r=70 entry=right label=Turnstile
```

`x`, `y` are the wheel center, `r` the distance from the center to the pockets
(default 70), `entry` the side of the entry position: `right` (flow right to left) or
`left`.

It generates the points `ts.in`, `ts.needle` and `ts.out`. Use `ts.in` and
`ts.out` in lanes, and write them **consecutively** in carrier routes
(`route=…,pit_in,ts.in,ts.out,pit_out,…`). A carrier is moved to `ts.in`
with a `move`, travels on the wheel only by `rotate`, and leaves with a `move`
from the exit (e.g. `to=merge`). `pipette` on the turnstile works at the
needle position.

Checked by `--check` (in time order, whatever the order of the steps in the file):
the entry pocket must be free when a carrier arrives, a carrier can leave only
from the exit position, a rotation can't start while a carrier is still in the
exit position, nothing arrives or leaves while the wheel is turning, and the wheel
doesn't turn until a carrier that has left (or is arriving) is completely outside
the wheel's circle.
See `scenes/02_turnstile_three_carriers.mmd`.

### Analyzer error and needle

`needle` moves the analyzer needle by hand (unlike `pipette`, which goes down
and back up by itself). While it is down and the analyzer is not in error the
probe shows "Aspirating…"; with `status` `error` it turns red with "Needle down".
`--check` refuses a turnstile rotation, or a carrier leaving a sampling point,
between a `needle down` and the end of the next `needle up`. See scenes 05 and 06.

### Spur

A spur is a U-shaped branch perpendicular to the Main-Lane, built from plain
points and lanes (see `scenes/09_spur_happy_path.mmd`): the NSD diverts the
carrier, without stopping, into a short piece of pit-lane that turns 90° into the
up branch; the sampling point is on the up branch with the analyzer on its right;
the carrier crosses at the top and comes back down the other branch to the
Main-Lane. Scenes 09–12 cover the happy path, Clear, analyzer error and run-away.

When the analyzer is beside the sampling point (not above it) the probe comes out
of the analyzer's side and "Aspirating…" / "Needle down" are written inside the
analyzer. If there is no room on the right of the analyzer, the `operator` works
from below it. Carrier tags never cover the analyzer. `nsd` and `sampling`
devices accept `label_x`, `label_y` to move their label.

### Spur with several needles

Scenes 13–16 show a spur serving **one analyzer with two sampling points** (the same
pattern works for more). Every `sampling` device is one needle of the analyzer:
needle 1 is the first one in the file. Each needle has its own pit-lane, leaving the
up branch to the right and rejoining it above its divert. All messages go to the
one analyzer actor; Sample Acquisition, Sample Complete and Status carry `Needle=n`.
Which needles a sample visits depends on its tests.

The entry of a pit-lane is a `divert` device (a standard divert): unlike the NSD
every carrier stops on its point and is read (the device turns blue), then it is
diverted or goes straight on while the next one stops behind. Use the point as a stop
in the routes and move the carrier off it with a `move`.

With several `sampling` devices the analyzer shows a band for each needle. A `status`
on a `sampling` device concerns that needle only: its band turns red or green and its
probe "Needle down", while the other needle keeps working. A `status` on the analyzer
concerns the whole instrument. A `zone` around a pit-lane can turn red alone.

### Ghost carriers (logical position)

A ghost carrier (`%% carrier ... ghost`, or `ghost=true`) shows where the interface *believes* a carrier is,
while physically it is somewhere else (e.g. Prepare was sent at the NSD, but the
carrier went straight on). It is drawn as a dashed outline under the real
carriers, with a dashed tag; antennas don't read it and gates don't open for it.
Use it together with the real carrier (same sample id, different id):
`hidden` (`visible=false`) + `show` to make it appear at the NSD, `hide` when it is
cleared. See scenes 03 and 04.

### Gate and antenna

`gate` and `antenna` are placed on a point (`at`) of a horizontal lane; both
work by themselves, without steps:

- **gate**: a barrier across the lane that lets one carrier through: it opens in
  0.22 s when the carrier held by it sets off (or just before one that does not stop
  reaches it) and closes in 0.22 s as soon as the carrier is through. The gate point doesn't need to be in the routes,
  only on their path.
- **antenna**: a flat reader on the lane, under the carriers; waves spread around a
  carrier while it is over the point, and `%% on <antenna>` starts a step at that
  moment (put the point in the routes). `gate=true` adds a gate right in front of a
  carrier standing on it (`<antenna>.gate`, across the lane in any direction): the
  belt never stops, the gate holds the carrier. `visible=false` makes it a trigger
  only. An nsd, sampling or divert device can carry its antenna and gate on the same
  line: `%% @device id=sp kind=sampling at=sampling antenna=antenna2 gate=true`. The
  scenes still write `%% on antenna2`; the layout image names them together,
  `sp.antenna2.gate`.

`--check` also refuses a carrier that drives into a turnstile while the analyzer
is pipetting there: it has to wait at the gate until the sampling is over.

When the sequence diagram has more rows than fit, it scrolls up as new rows appear.
If `--check` finds an error, it also prints the timeline up to that point.

Easing tip: `ease=in` followed by `ease=out` gives one continuous
motion with no stop at the junction. Scene 01 uses this to pass through the
NSD without stopping.
