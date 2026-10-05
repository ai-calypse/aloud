import type { Post } from "../posts.ts";

const DICTATION = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";
const VC_ON = "https://support.apple.com/guide/mac-help/mchl63d14732/mac";
const VC_COMMANDS = "https://support.apple.com/guide/mac-help/use-voice-control-commands-mh40719/mac";
const VC_CUSTOM = "https://support.apple.com/guide/mac-help/customize-voice-control-mchl9899c8a5/mac";
const VC_VOCAB = "https://support.apple.com/guide/mac-help/use-a-custom-vocabulary-mchl3eb7b79a/mac";
const ACCESS = "https://support.apple.com/guide/mac-pro/apd4c143b3c5/mac";
const SIRI = "https://support.apple.com/en-au/guide/mac-help/mchl6b029310";
const AVAIL = "https://www.apple.com/macos/feature-availability/";

export const post: Post = {
  slug: "dictation-vs-voice-control-mac",
  title: "dictation vs voice control on mac: which one do you need?",
  metaTitle: "Dictation vs Voice Control on Mac: Which to Use",
  description: "Mac Dictation switches off while Voice Control is on. Compare both and Siri in one table, with the 30-second silence timer and 1,000-term vocabulary cap.",
  date: "2026-10-05",
  checked: "2026-10-05",
  minutes: 6,
  tags: ["voice control mac", "dictation vs voice control", "mac voice commands"],
  cover: { src: "/blog/dictation-vs-voice-control-mac.webp", alt: "Pixel art with a white card holding a microphone on the left, a card with a grid of squares and a mouse pointer on the right, and an orange cat between them" },
  blocks: [
    { p: `Apple says it plainly: "when Voice Control is on, you use Voice Control to dictate text; standard macOS Dictation isn't available" ([Apple Support](${VC_COMMANDS})). if your mac's dictation shortcut suddenly does nothing, that sentence is the first thing to check.` },
    { p: "dictation, Voice Control and Siri all listen to your voice, so they get mixed up constantly. they do different jobs. this guide compares them side by side, using Apple's own documentation, and tells you which to reach for." },
    {
      takeaways: [
        `Dictation turns speech into text anywhere you can type ([Apple](${DICTATION})).`,
        `Voice Control lets you navigate the desktop, click, and dictate and edit text by voice ([Apple](${VC_ON})).`,
        "they conflict: while Voice Control is on, standard Dictation isn't available.",
      ],
    },
    { cta: { slot: "intro", button: "just want to type by voice? try SpeakOS" } },

    { h: "what's the difference between dictation and voice control on a mac?" },
    { p: `Dictation is for text. Apple describes it as speaking "to enter text anywhere you can type it" ([Apple Support](${DICTATION})). Voice Control is for operating the whole mac: you can "navigate the desktop and apps, interact with what's on the screen, dictate and edit text, and more" ([Apple Support](${VC_ON})).` },
    { p: `Siri is a third thing. Apple calls it "an intelligent assistant that's built into your Mac" that helps you "find the information you need and get things done" ([Apple Support](${SIRI})). Apple files Siri and Voice Control in separate parts of its guide.` },
    {
      table: {
        caption: "Dictation, Voice Control and Siri, from Apple's documentation",
        head: ["", "Dictation", "Voice Control", "Siri"],
        rows: [
          ["main job", "enter text", "control the mac and apps, plus dictate and edit text", "answer questions and run tasks"],
          ["turn it on", "System Settings, Keyboard, Dictation", "System Settings, Accessibility, Voice Control", "see Apple's Siri guide"],
          ["start it", "Microphone key, shortcut, or Edit, then Start Dictation", "toggle it on, then it keeps listening", "see Apple's Siri guide"],
          ["time limit", "no limit, but stops after 30 seconds of silence", "stays on until you turn it off or say \"Stop listening\"", "not covered"],
          ["offline", "check Keyboard settings for on-device processing", "one-time download, then no internet needed", "not verified"],
          ["languages", "more than 50", "about 20", "see Apple"],
        ],
      },
    },
    { p: `sources: [Dictation](${DICTATION}), [Voice Control](${VC_ON}) and [Siri](${SIRI}) in Apple's Mac User Guide. language lists are on [Apple's feature availability page](${AVAIL}), where Dictation lists more than 50 languages and Voice Control about 20.` },

    { h: "when should you use Dictation?" },
    { p: `whenever the job is writing. Dictation puts words into an email, a doc or a chat, and you can say punctuation and formatting such as "new line" and "new paragraph" ([Apple Support](${DICTATION})). on a mac with Apple silicon you can keep using the keyboard while you speak, so there's no need to stop dictation. to turn it on, open System Settings, click Keyboard in the sidebar, switch on Dictation and click Enable. Apple says you can dictate "text of any length without a timeout", and you can check in Keyboard settings whether your voice is processed on your device. if the writing is email, see our [guide to dictating emails on a mac](/blog/how-to-dictate-emails-on-mac).` },
    { p: "the trade-off is the timer: dictation ends after 30 seconds with no speech. our [dictation not working guide](/blog/mac-dictation-not-working) covers that, and the [shortcut guide](/blog/mac-dictation-shortcut) covers starting and stopping." },
    { img: "/blog/cat-listening-cat.webp", alt: "A white cat with its paws folded and the words I am listening over the picture", caption: "Voice Control, on its best behavior.", width: 520, height: 520 },

    { h: "when should you use Voice Control?" },
    { p: `when you want to operate the mac without a keyboard or mouse. you can say commands like "Open Mail", "Scroll Down" or "Click Done", and Voice Control can overlay numbers, names or a grid on the screen so you can point at anything ([Apple Support](${VC_COMMANDS})). it has three text modes: Dictation mode, Spelling mode and Command mode.` },
    { p: `it's also customizable. most commands are on by default, and you can add your own. you can add up to 1,000 vocabulary terms in a language that supports it ([Apple Support](${VC_CUSTOM}), [vocabulary](${VC_VOCAB})). Apple files Voice Control under Accessibility, in the Mobility category ([Apple Support, Mac Pro guide](${ACCESS})).` },
    {
      cta: {
        slot: "mid",
        title: "not looking to run your whole mac by voice?",
        text: "SpeakOS is simpler: hold a key, say it, and clean text lands where your cursor is. no overlays, no commands to learn.",
        button: "try hold-to-talk on my mac",
      },
    },

    { h: "how do you turn Voice Control on and off?" },
    { p: `open System Settings, click Accessibility in the sidebar, choose Voice Control, and switch it on or off. you can also tell Siri "Turn on Voice Control" ([Apple Support](${VC_ON})). the first time you turn it on, your mac must be online for a one-time download from Apple. after that you don't need an internet connection.` },
    { p: `to pause it, say "Stop listening", and say "Start listening" to resume. older guides and blog posts may still teach "Go to sleep" and "Wake up". Apple's current Voice Control page uses "Stop listening" and "Start listening" instead, so use those ([Apple Support](${VC_COMMANDS})).` },
    { p: `Apple also says all audio processing for Voice Control happens on your mac ([Apple Support, Mac Pro guide](${ACCESS})). that line comes from the macOS Tahoe 26 accessibility guide for Mac Pro, so check your own version's page if privacy matters to you.` },

    { h: "why did dictation stop working after I turned on Voice Control?" },
    { p: `because they're mutually exclusive, and Apple's dictation guide repeats the rule: when Voice Control is enabled, standard macOS Dictation isn't available ([Apple Support](${DICTATION})). switch Voice Control off in Accessibility and the dictation shortcut should start working again.` },
    { p: `if you do want both abilities, Voice Control has its own Dictation mode, so you can dictate text without leaving it. it also has Spelling mode, which enters text letter by letter, and Command mode, which treats everything you say as a command ([Apple Support](${VC_COMMANDS})). say "Stop listening" to pause Voice Control and "Start listening" to resume.` },
    { img: "/blog/cat-hand-on-computer.webp", alt: "A giant hand reaching over a beige retro computer keyboard", caption: "what Voice Control feels like on day one.", width: 640, height: 800 },

    { h: "which should you pick?" },
    {
      table: {
        caption: "Pick by the job",
        head: ["you want to", "use", "why"],
        rows: [
          ["write an email, doc or message by voice", "Dictation", "it's built for entering text"],
          ["use your mac with no keyboard or mouse", "Voice Control", "it can click, scroll and open apps by command"],
          ["ask a question or run a quick task", "Siri", "it's an assistant, not a text tool"],
          ["type by voice without learning commands", "hold-to-talk (SpeakOS)", "one key, no overlays, no timer. our product, so weigh that"],
        ],
      },
    },
    { p: "if you mainly want to write, start with Dictation, and if the 30-second timer bothers you, a held key removes it. the [how to dictate on a mac guide](/blog/how-to-dictate-on-a-mac) starts from zero." },
    { cta: { slot: "end", title: "write by voice, skip the setup", text: "hold the key, say it, let go. SpeakOS puts clean text into whatever app you're in.", button: "grab SpeakOS for my mac" } },

    {
      faq: [
        ["what is the difference between dictation and voice control on mac?", `Dictation enters text by voice. Voice Control also navigates the desktop, clicks and edits, and includes its own dictation ([Apple Support](${VC_ON})). Apple says standard Dictation isn't available while Voice Control is on.`],
        ["how do you turn off voice control on a mac?", `open System Settings, Accessibility, Voice Control and switch it off ([Apple Support](${VC_ON})). that also frees standard Dictation to work again.`],
        ["does voice control work offline?", `yes, after a one-time download. Apple says your mac must be online the first time you turn it on, and after that you don't need an internet connection ([Apple Support](${VC_ON})).`],
        ["can you dictate while voice control is on?", `yes, but through Voice Control's own Dictation mode. Apple says standard macOS Dictation isn't available while Voice Control is on ([Apple Support](${VC_COMMANDS})).`],
        ["is voice control the same as siri?", `no. Siri is "an intelligent assistant that's built into your Mac" ([Apple Support](${SIRI})), while Voice Control lets you "navigate the desktop and apps, interact with what's on the screen, dictate and edit text" ([Apple Support](${VC_ON})). Apple documents them in separate sections.`],
      ],
    },
  ],
};
