# trackviz — scene tables and devices

`README.md` explains how to write a scene with the Mermaid directives. This file is the
reference underneath them: the `%% @<section>` tables that the directives produce and
that the geometries in `scenes/common/` are written with, and how each kind of device
works. You need it to change a geometry or to make a new one, not to write a scene.

## Tables

A scene is a Mermaid sequence diagram (`.mmd`). Besides the directives (see `README.md`), a
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
| `@video` | `title`, `subtitle`, `width`, `height`, `fps`, `tail` (seconds held at the end), `show_clock`, `show_times`, `belt` (cm/s, default 17.5) |
| `@point` | `id`, `x`, `y` |
| `@lane` | `id`, `label`, `kind` = `main` \| `pit` \| `link`, `points` = list of ids, `label_x`, `label_y`, `capacity` (most carriers in it at once, see Pit-lane capacity in `README.md`) |
| `@device` | `id`, `kind` = `nsd` \| `sampling` \| `divert` (need `at` = point id) \| `analyzer` \| `zone` (need `x`,`y`,`w`,`h`) \| `turnstile` (see below) \| `gate`, `antenna` (need `at`, see below), `label` |
| `@actor` | `id`, `label`, `sub`, `color` (sequence-diagram lifelines, left to right), `analyzer` (this actor gets the "Pipetting" note) |
| `@message` | `id`, `from`, `to`, `text`, `params`, `reply`, `color` (named message, see below) |
| `@carrier` | `id`, `tube`, `sample_id`, `route` = list of points and lane ids (see Routes in `README.md`), `start`, `speed` (px/s), `cap_color`, `ghost` (see below), `visible` (default true), `tag` (tag text, default the SampleId) |
| `@step` | `action`, one of `start` / `at` / `after_start` / `after_end` / `when`, `delay`, `duration` + action keys (below) |

## Steps

These are the keys of a `@step` table, what the directives produce: `on antenna1` gives
`when="C1 at nsd"`, `after 0.2 pipette sp` gives `when="pipette sp done" delay=0.2`,
`after 1` gives `after_end=1`. Written by hand in `@step` lines they still work. Rows of
the sequence diagram always follow time, whatever the order in the file.

`start`:

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
| `pipette` | `device` (a `sampling` or `turnstile` device), `sample_id`, `note` (what they do: `pipette` in `README.md`) | 3 s |
| `highlight` | `device` (not needed when an antenna reads a carrier: see `README.md`) | 1.5 s |
| `rotate` | `device` (a `turnstile`) | 2 s |
| `show` / `hide` | `carrier` (fades it in / out) | 0.5 s |
| `status` | `device` (an `analyzer`, a `zone`, or a `sampling` device for one needle), `state` = `error` (red; ERROR badge on an analyzer) \| `ready` (green) \| `normal` (grey); lasts until the next `status` | 0 s |
| `operator` | `device` (an `analyzer`): a stylized lab operator (white coat, thin blue gloves) walks in from the right edge of the panel, works on the analyzer with one hand, then walks away (walks take up to 2 s each) | 7 s |
| `needle` | `device` (a `sampling` or `turnstile`), `to` = `down` \| `up`: the needle goes down and stays there until `up` | 0.9 s |
| `wait` | — | `duration` is required |

`{SampleId}` and `{CarrierId}` inside `text` and `params` are replaced with the
carrier's values, `{Needle}` with the `needle` key of the step (required then).

## Named messages

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

## Turnstile

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
exit position, and nothing arrives or leaves while the wheel is turning.

A `rotate` waits by itself until the wheel is free: the previous rotation is over,
a carrier driving into the entry is in its pocket, and a carrier that has just left
is completely outside the wheel's circle. The log of `--check` shows the wait
("waits 0.5s for the wheel to be clear"), so a rotation can follow the departure of
the carrier at any belt speed. A carrier moved later in the file that would still
touch the turning wheel is an error.
See `scenes/02_turnstile_three_carriers.mmd`.

## Analyzer error and needle

`needle` moves the analyzer needle by hand (unlike `pipette`, which goes down
and back up by itself). While it is down and the analyzer is not in error the
probe shows "Aspirating…"; with `status` `error` it turns red with "Needle down".
`--check` refuses a turnstile rotation, or a carrier leaving a sampling point,
between a `needle down` and the end of the next `needle up`. See scenes 05 and 06.

## Spur

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

## Spur with several needles

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

## Ghost carriers (logical position)

A ghost carrier (`%% carrier ... ghost`, or `ghost=true`) shows where the interface *believes* a carrier is,
while physically it is somewhere else (e.g. Prepare was sent at the NSD, but the
carrier went straight on). It is drawn as a dashed outline under the real
carriers, with a dashed tag; antennas don't read it and gates don't open for it.
Use it together with the real carrier (same sample id, different id):
`hidden` (`visible=false`) + `show` to make it appear at the NSD, `hide` when it is
cleared. See scenes 03 and 04.

## Gate and antenna

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
