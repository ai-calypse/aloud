# SpeakOS for iPhone — UI and user flows

SwiftUI prototype of the SpeakOS iPhone UI, built from `speakos_ios_ui_creation_guide.md`
and the final logo. **UI only:** no audio, no network, no accounts, no model. Every outcome is a
simulated fixture, so this is a clickable prototype, not an app that records or sends anything.

## Run it

Needs Xcode 15+ (iOS 17 deployment target) and [XcodeGen](https://github.com/yonaskolb/XcodeGen).

```sh
cd apps/ios
brew install xcodegen   # once
xcodegen                # writes SpeakOS.xcodeproj (gitignored)
open SpeakOS.xcodeproj
```

Built with the iOS 26 SDK, the tab bar and sheets pick up Liquid Glass on their own; the app
draws no glass of its own.

> ⚠️ **Not compiled yet.** This was written in a Linux container with no Swift toolchain.
> Expect a first-build pass to fix small compiler errors before it runs, and nothing here has been
> seen on a simulator or device.

## Screens and flows

Tabs: **Memory · Speak · Meetings**, Speak first. Settings is top-right. All seven screen families
from the guide exist; the nine flows are reachable like this.

| Flow | Try it |
| --- | --- |
| F01 speak and remember | Hold the logo → release. First sample is a thought. |
| F02 speak and compose | Set *next voice sample* to Draft (Settings → prototype controls), or type "write Sarah a message saying …". |
| F03 connected action | Sample *Slack action* → pick Sarah → review → send. Try *next send* = fails once / outcome unknown, or set Slack to *needs reconnecting*. |
| F04 memory question | Sample *Memory question* → open a source → back returns to the answer. |
| F05 browse and search | Memory tab → search and filter → open → back keeps query and filter. |
| F06 meeting | Speak → *start meeting* → start recording → pause/finish → notes. |
| F07 recover a meeting | *Customer call* in Meetings is an unfinished session; or turn on *interrupt next meeting*. |
| F08 type instead | *type instead* on Home. Typed text takes the same path as voice. |
| F09 manage connections | Settings → Connected apps → Notion (starts unconnected). |

Settings → **prototype controls** switch every state in the guide on demand: first-use account,
microphone ask/denied, a failed capture, unavailable search, send outcomes, connection results,
meeting interruption and failed notes. They simulate outcomes and would not ship.

## Layout

```
project.yml            XcodeGen spec
SpeakOS/
  Design/              Theme (palette, type), Components
  Model/               value types, Interpreter (intent stand-in), AppModel, Fixtures
  Navigation/          tabs, the one AppSheet enum, RootView
  Features/            Speak, Sheets, Memory, Meetings, Settings
  Assets.xcassets/     SpeakosLogo + AppIcon, both the supplied logo, unmodified
SpeakOSTests/          Interpreter and AppModel behaviour
```

`AppModel` is one `@Observable` store; `AppSheet` keeps a single modal on screen at a time.

## Decisions worth knowing

- **Look:** night-sky ink and cool paper, drawn from the logo. The signature is the *sky window*:
  the supplied art runs edge to edge across the top of Home as the hold-to-talk target. It dims
  under a night veil while listening and a pixel-block meter rises in front of it. A small orange
  pixel marks section headings, and nothing else repeats it. This replaces the guide's warm cream
  canvas, which read as a generic template next to the artwork.
- **Type:** Bricolage Grotesque for headings and key labels, Geist for reading, Geist Mono for
  small status labels (all OFL, bundled in `Resources/Fonts`, registered at launch). They are
  variable fonts: if a face falls back to the system font on device, check `UIFont.familyNames`
  against the names in `Theme.swift`.
- **Logo:** the supplied SVG, rasterised once at 1024 px, kept pixel-sharp (`interpolation(.none)`)
  and never redrawn or animated. It is opaque, so it is used as a full-bleed image, not a cut-out.
  The app icon is the same artwork.
- **Light appearance only** (`preferredColorScheme(.light)`), as the guide specifies. A reviewed
  dark pass is open work, and Apple's guidance is not to ship an app-only appearance switch.
- **Voice is simulated:** holding plays a scripted sentence into the live transcript.
- **Intent is rules, not a model** (`Interpreter`): remember → thought, write → draft, send/email/post
  → action, question → answer, otherwise it asks one question. Answers only quote saved items.
- **Orange and yellow are fills under ink labels**, never text on white. At most one orange button per screen.

## Not done

Real audio, speech-to-text, the model, Slack/Gmail/Notion, persistence (everything resets on
relaunch), dark mode, iPad, large-text and contrast verification on device, VoiceOver pass, and the
guide's usability walkthrough (S04-T02). The guide's open decisions (gesture threshold, large-text sheet
behaviour, background recording) are unresolved: the recording UI shows interruption honestly and
promises no background capture.
