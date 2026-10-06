import type { Post } from "../posts.ts";

const APPLE_FIX = "https://support.apple.com/guide/mac-help/mchlc480652b/mac";
const APPLE = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";
const APPLE_SOUND = "https://support.apple.com/guide/mac-help/change-the-sound-input-settings-mchlp2567/mac";
const APPLE_MIC = "https://support.apple.com/guide/mac-help/control-access-to-your-microphone-on-mac-mchla1b1e1fe/mac";
const DOCS = "https://support.google.com/docs/answer/4492226?hl=en";
const DOCS_KEYS = "https://support.google.com/docs/answer/179738";
const CHROME_MIC = "https://support.google.com/chrome/answer/2693767?hl=en";

export const post: Post = {
  slug: "mac-dictation-not-working",
  title: "mac dictation not working? start with Apple's checklist.",
  metaTitle: "Mac Dictation Not Working? 10 Fixes From Apple's Guide",
  description: "Mac dictation stops after 30 seconds of silence and isn't available while Voice Control is on. Ten fixes from Apple's guide, plus Google Docs and Chrome.",
  date: "2026-10-05",
  checked: "2026-10-05",
  minutes: 6,
  tags: ["mac dictation not working", "dictation keeps stopping mac", "mac dictation external microphone"],
  cover: { src: "/blog/mac-dictation-not-working.webp", alt: "Pixel art of an orange cat sitting beside a laptop that shows two red exclamation marks, with a crossed-out microphone above them" },
  blocks: [
    { p: `"dictation stops automatically when no speech is detected for 30 seconds" ([Apple Support](${APPLE})). that one sentence is the first thing to rule out when mac dictation is not working. you paused to think, the clock ran out, and dictation quietly stopped.` },
    { p: "mac dictation is the speech-to-text feature built into macOS, controlled from Keyboard settings. it isn't the only cause. standard dictation isn't available while Voice Control is on, and it can pick the wrong microphone. this guide goes through Apple's own troubleshooting list first, then the fixes for an external mic, Google Docs and Chrome." },
    {
      takeaways: [
        `dictation stops after 30 seconds without speech ([Apple](${APPLE})).`,
        `Apple's troubleshooting page covers ten fixes, from language to microphone volume ([Apple](${APPLE_FIX})).`,
        "Google Docs has its own voice typing, separate from macOS dictation, with its own microphone permission.",
      ],
    },
    
    { h: "why does mac dictation keep stopping?" },
    { p: `because Apple built it to. dictation stops automatically after 30 seconds with no speech, and it can run for any length otherwise ([Apple Support](${APPLE})). the Dictation page we checked lists no other automatic stop. so if it quits mid-thought, count the silence before you blame the settings.` },
    { p: `to stop it yourself, press the Escape key, the Microphone key or your Dictation shortcut ([Apple Support](${APPLE})).` },
    { p: "the practical fixes are small. keep talking through the pause, say your next sentence out loud even if it's rough, or restart with the shortcut when it drops. if you're stuck on the shortcut itself, see the [mac dictation shortcut guide](/blog/mac-dictation-shortcut)." },

    { cta: { slot: "intro", button: "hit the 30-second wall? try hold-to-talk" } },

    { h: "what's on Apple's dictation troubleshooting checklist?" },
    { p: `Apple's page for dictation problems lists its fixes as short tips ([Apple Support](${APPLE_FIX})). we've numbered them and kept Apple's order. the first three are quick settings checks.` },
    {
      ol: [
        "turn Dictation on in Keyboard settings.",
        "choose the correct language and region.",
        "use the correct Dictation keyboard shortcut.",
        "no internal microphone? connect an external one and select it in Sound or Keyboard settings.",
        "with an external microphone, make sure its input volume is sufficient.",
        "keep the microphone clear of clothing, your body and other objects.",
        "sit where you normally use your Mac, and speak clearly at a steady volume.",
        "cut background noise. in a noisy or echoey room, a headset microphone may help.",
        "check Network settings, because you may need an internet connection.",
        "check Keyboard settings to see whether voice input is processed on your device and not sent to Siri servers.",
      ],
    },
    { p: `one cause isn't on that list. when Voice Control is on, you use Voice Control to dictate text, and standard macOS Dictation isn't available ([Apple Support](${APPLE})).` },
    { img: "/blog/cat-error-cat.webp", alt: "A cat in sunglasses and a headscarf sipping from a mug in a deckchair while the house behind it burns", caption: "your dictation at second 31, probably.", width: 520, height: 514 },
    {
      cta: {
        slot: "mid",
        title: "no toggle, no 30-second clock",
        text: "SpeakOS records while you hold the key and stops when you let go, so a pause to think never ends it. clean text lands in whatever app has focus.",
        button: "try it on my mac",
      },
    },

    { h: "why does dictation fail with an external microphone?" },
    { p: `usually because your Mac is listening to a different microphone than you think. there are three places to check, and Apple documents all of them: the microphone source for Dictation, the system input device, and the input volume ([Apple Support](${APPLE_SOUND})).` },
    {
      table: {
        caption: "Three places a microphone gets chosen",
        head: ["where", "what to check", "source"],
        rows: [
          ["System Settings, Keyboard, Dictation", "the Microphone source. Automatic means your Mac listens to the device you're most likely to use.", `[Apple](${APPLE})`],
          ["System Settings, Sound, Input", "select the device you want in the list of sound input devices.", `[Apple](${APPLE_SOUND})`],
          ["Sound, Input, volume slider", "raise it for a quiet voice, lower it for a loud source. for an external microphone, Apple says to make sure its input volume is sufficient.", `[Apple](${APPLE_SOUND}), [Apple](${APPLE_FIX})`],
        ],
      },
    },
    { p: `one more place matters if an app can't hear you at all. open System Settings, Privacy & Security, Microphone, and check that the app is switched on ([Apple Support](${APPLE_MIC})). the Recording Indicator in Control Center shows when the microphone is in use or has been used recently.` },
    { p: `if the microphone keeps reverting to a headset, Apple says to unplug or disconnect the competing device before you select the one you want ([Apple Support](https://support.apple.com/guide/mac-help/mac-switches-a-audio-device-mh35883/mac)).` },

    { h: "why is dictation not working in Google Docs or Chrome?" },
    { p: `because Google Docs has its own voice typing, and it isn't macOS dictation. in Docs you open Tools, then Voice typing, and a microphone box appears ([Google Docs Help](${DOCS})). the keyboard shortcut on a Mac is ⌘ + Shift + S ([Google shortcuts](${DOCS_KEYS})).` },
    { p: `two things trip people up here. first, Google's pages disagree on browsers: the voice typing page lists Chrome, Edge and Safari, while the shortcuts page says "available in Chrome browsers" ([Google Docs Help](${DOCS})). second, voice commands only work in English, and both your account language and the document language must be English.` },
    { p: `if the box asks for the microphone and nothing happens, check the site permission in Chrome: Settings, Privacy and security, Site settings, Microphone, then set the site to Allow ([Chrome Help](${CHROME_MIC})). that is a separate switch from the macOS one above.` },

    { h: "do you need internet for mac dictation?" },
    { p: `maybe. Apple's current guide says "you may need an internet connection" and points you to Keyboard settings to see whether input is processed on your device ([Apple Support](${APPLE_FIX})). an older Apple guide for macOS 14 ([Apple Support](https://support.apple.com/guide/mac-help/use-dictation-mh40584/14.0/mac/14.0)) was blunter: on Apple silicon, supported languages are processed on the device, while an Intel Mac or an unsupported language sends your words to Apple. Apple's current guide scopes the on-device check to general text dictation, such as messages and notes ([Apple Support](${APPLE_FIX})).` },
    { p: "treat that as a reason to check, not to assume. the [how to dictate on a mac guide](/blog/how-to-dictate-on-a-mac) shows where to look, and our [privacy page](/privacy) says what SpeakOS keeps on your mac." },

    { h: "when is a different approach worth it?" },
    { p: "if you hit the 30-second wall every day, the toggle model is the problem. hold-to-talk fixes it by design: you hold a key while you speak and let go when you're done, so thinking time doesn't count against you." },
    { p: "SpeakOS is our Mac dictation app and it works that way, so weigh that. the built-in dictation is free and fine for short notes. we'd only suggest switching if you keep losing sentences to a timeout." },
    { cta: { slot: "end", title: "keep your sentences", text: "hold the key, say the whole thing, let go. text appears where your cursor is.", button: "grab SpeakOS for my mac" } },

    {
      faq: [
        ["why does dictation stop on mac?", `Apple documents one automatic stop: dictation ends after 30 seconds with no speech ([Apple Support](${APPLE})). pressing Escape, the Microphone key or your shortcut stops it too.`],
        ["why is mac dictation not working in Google Docs?", `Docs uses its own voice typing under Tools, then Voice typing ([Google Docs Help](${DOCS})), with ⌘ + Shift + S as the shortcut ([Google shortcuts](${DOCS_KEYS})). check Chrome's site microphone permission as well.`],
        ["does dictation work with an external microphone?", `yes. choose the device in System Settings, Sound, Input, and set the Dictation microphone source in Keyboard settings. Apple says to make sure an external microphone's input volume is sufficient ([Apple Support](${APPLE_FIX})).`],
        ["does mac dictation work in Terminal?", "we couldn't find Apple or Google documentation either way, so we won't guess. try a plain text field first to confirm dictation works, then "],
        ["why won't dictation start at all?", `check that it's on, that the language is right, and that Voice Control is off. Apple says standard Dictation isn't available while Voice Control is on ([Apple Support](${APPLE})). our [Dictation vs Voice Control comparison](/blog/dictation-vs-voice-control-mac) explains the difference.`],
      ],
    },
  ],
};
