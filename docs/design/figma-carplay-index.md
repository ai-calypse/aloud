# CarPlay Design System: the widget, system and colour index

What the notch's "personality" pass takes from the Figma file, with the values
read from the file, not guessed from screenshots.

- **File:** CarPlay Design System (Community), fileKey `kLPrZ1jNJLJzH6ZpqQY7rz`,
  page "CarPlay Screens" (`33:2373`), read 4 October 2026 through the Figma MCP.
- **Method:** `get_variable_defs` on the widget library and system components,
  `get_screenshot` on the gauge dashboards and the large calendar, and the page's
  layer tree for names and sizes. Apple publishes this as design resources; the
  colours below are Apple's system colours, which a native Mac app can use freely.
- **What is not in it:** the circular gauges and rings (Dashboard `189:13295`,
  `189:13403`) use raw fills with no variables, so their colours below are read
  from the render, not from tokens.

## The library: where things live

| Node | What it is |
|---|---|
| `109:53998` Small Widget, Homescreen, Dark (and `109:53892` Medium, `109:53798` Large, plus Light twins) | Apple's widget set, one strip per size: Batteries, Calendar, Clock, Contacts, Files, Find My, Fitness, Home, Maps, Music, News, Notes, Photos, Podcasts, Reminders, Safari, Screen Time, Shortcuts, Siri Suggestions, Sleep, Stocks, Tips, TV, Wallet, Weather. This is the token source. |
| `109:41334` Large / Calendar (329×334) | Day number, month grid with a red today dot, event cards with a coloured leading bar |
| `109:40697` Medium / Weather (329×155) | |
| `109:41513` Large / Music (329×334) | |
| `109:42812` Batteries, Medium (338×158) | |
| `189:13295`, `189:13403` Dashboard (1462×465) | Instrument gauges: left speed ring, centre map, right power ring |
| `189:13165` Display, `189:13139` Media, `189:13145` Fuel, `189:13230` Weather | Car display screens |
| `108:35468` Snackbar, `108:35607` Modal-popup, `108:35928` System-control | System UI: toast, dialog, control strip |
| Toggle `33:2456`, Big-Button, Settings-list, Back-button, Bottom-Tray, Icon-with-label, System Tray, Pagination | Controls |

## Colour (dark appearance)

**Surfaces**

| Role | Value |
|---|---|
| Tile background (Calendar, Reminders, Notes, Wallet, Screen Time) | `#1C1C1E` |
| Vibrant tile background (Batteries, Contacts, Files, Clock, Siri) | `#252525` |
| Sleep background | `#000000` |
| Neutral container / bar track | `#48484A`, `#2C2C2E`, `#292929` |
| Separator, grid line | `#323234`, `#504F54` |
| Unselected radio | `#464648` |

**Text**

| Role | Value |
|---|---|
| Primary | `#FFFFFF` |
| Secondary | `#8E8D93` (`#8D8D92`, `#939298` in places) |
| Tertiary | `#69686D` |
| Axis label | `#9999A1` |
| Label secondary (system) | `#EBEBF5` |

**Accents by app, which is where the personality is**

| App | Accent | Detail |
|---|---|---|
| Calendar, Reminders | `#FF4539` red | day of week and today dot |
| Calendar events | Green `#97E37D` on `#1F4013`, time `#7ABD62`, indicator `#9AE77E`. Yellow `#F5D04B` on `#4B3D04`, time `#C9AA3A`, indicator `#FFDA50` | event title, time, card fill |
| Batteries / healthy | `#34C85A` charge (system green `#34C759`) | track `#D9D9D9`, primary `#E6EDED` |
| Fitness rings | Move `#F9104F` (track `#3A171D`), Exercise `#A7FE01` (track `#1E3719`), Stand `#00FFF5` (track `#163239`) | the pattern: bright metric over a track of the same hue at ~15% |
| Screen Time | `#0983FE`, `#3FC7DF`, `#FEA00A` | chart categories |
| Sleep | `#63E6E1` | |
| Music | Button `#FF4F65`, metadata `#FDCDE5` | |
| Podcasts | Metadata `#FEAAFF`, pill `#C340FF` | |
| Weather | Sun `#FFD409` | |
| Maps | Current location `#0A84FF` | |
| Find My | Location overlay `#65CDA6` | |
| Stocks | Positive `#34C85A` | |
| Warning yellow (modal) | `#FFCC00` | |
| Mint (system control) | `#00C7BE` | |

**Gauge dashboards (from the render, no tokens):** each dashboard is themed by one
hue. The speed and power rings are peach (`~#EFD0B8`) on a charcoal disc
(`~#2A2A2C`), inside a deep red-brown tile (`~#4A1308`), with a smaller
darker-red centre (`~#5A3128`) carrying the big number. A temperature arc runs
green → yellow → orange. So: **one theme hue per dashboard, the ring is the
lightest tint, the tile the deepest shade, numerals in the lightest tint.**

## Type

The file uses SF Pro (Text, Display, Rounded). Sizes worth keeping:

| Use | Spec |
|---|---|
| Big number, battery | Display Regular 46 |
| Big number, weather | Display Light 45 |
| Big number, stocks | Regular 38, tracking -0.1 |
| Calendar date | Display Light 32 |
| Title 1 (modal, snackbar) | Regular/Bold 28, line 34, tracking -0.4 |
| Event / row title | Text Semibold 13 |
| Time / label | Text Medium 12 |
| Overline, goal | Text Semibold 11 |
| Reminder task | Rounded Regular 13, tracking +0.5 |
| Wallet balance | Rounded Bold 18 |

Light numerals (300) at 32–45px are what makes the big numbers feel Apple.
Aloud's notch tokens use Geist; a native Mac app can use `-apple-system` (SF)
for free. That is a design choice for the user: Geist everywhere, or SF for the
numerals only.

## Sizes

Large widget 329×334, medium 329×155 or 338×158, mini calendar 142×129, snackbar
663×122–238, modal 663×415. Tiles are rounded ~20pt on black. The notch panel is
780px, so a 3-up row of ~240px tiles or a 2-up of ~370px reads like these.

## Mapping to the notch

| Notch surface | Figma source |
|---|---|
| Today › Calendar | Large / Calendar: red date, green and yellow event cards with a leading bar |
| Today › Reminders | Reminders: red accent, `#464648` radio, `#323234` separators |
| Today › Weather | Medium / Weather: light 45px temperature, sun `#FFD409` |
| Today › Stocks | Stocks: 38px number, green positive `#34C85A` |
| Mac › Power, Devices | Batteries: capsule with `#34C85A` charge on `#252525` |
| Mac › Stats | Fitness rings: bright metric over its own dark track |
| Focus › Timer | One of the three Fitness rings (Move red `#F9104F`) draining |
| Media | Music: `#FF4F65` button, `#FDCDE5` metadata; tint from album art |
| Agents › Usage | Screen Time bars: `#0983FE`, `#3FC7DF`, `#FEA00A` |
| Toasts and chips | Snackbar |
| Approvals, confirmations | Modal-popup |

## Real app icons

The prototype does not need them. In the real app, `aloud_selection::app_icon`
already reads each Mac app's own icon at runtime, and `aloud_agent::icons` caches
connected services' logos, so nothing needs to be bundled.
