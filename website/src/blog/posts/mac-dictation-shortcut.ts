import type { Post } from "../posts.ts";

const APPLE = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";
const APPLE_26 = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/26/mac/26";
const APPLE_14 = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/14.0/mac/14.0";
const APPLE_KEYS = "https://support.apple.com/en-us/102650";
const APPLE_COMMANDS = "https://support.apple.com/guide/mac-help/commands-for-dictating-text-on-mac-mh40695/mac";
const APPLE_FN = "https://support.apple.com/en-us/102439";
const APPLE_CUSTOM = "https://support.apple.com/guide/mac-help/create-keyboard-shortcuts-for-apps-mchlp2271/mac";
const APPLE_FIX = "https://support.apple.com/guide/mac-help/mchlc480652b/mac";
const APPLE_VC = "https://support.apple.com/guide/mac-help/use-voice-control-commands-mh40719/mac";

export const post: Post = {
  slug: "mac-dictation-shortcut",
  title: "mac dictation shortcut: set it, change it, fix it.",
  metaTitle: "Mac Dictation Shortcut: Set, Change and Fix It",
  description: "Apple gives you 3 ways to start Mac dictation: the Microphone key, a keyboard shortcut or the Edit menu. How to set it, change it, stop it and fix it.",
  date: "2026-10-05",
  checked: "2026-10-05",
  minutes: 6,
  tags: ["mac dictation shortcut", "mac dictation shortcut not working", "dictation commands mac"],
  cover: { src: "/blog/mac-dictation-shortcut.webp", alt: "Pixel art of a row of keyboard keys labelled FN, FN, F5, ESC and Z with the F5 key highlighted in yellow under a microphone, and an orange cat sitting beside them" },
  blocks: [
    { p: `the mac dictation shortcut Apple lists is Fn-D, which starts or stops dictation ([Apple Support](${APPLE_KEYS})). you can also press the Microphone key or choose Edit, then Start Dictation ([Apple Support](${APPLE})), and you may be able to set a different shortcut in Keyboard settings.` },
    { p: "this guide covers what Apple documents: where to set the shortcut, how to change it, how to stop dictation, and the usual reasons the shortcut does nothing. where Apple's pages are silent, we say so instead of guessing. we make SpeakOS, a mac dictation app, so we also say when a held key beats a toggle." },
    {
      takeaways: [
        `three ways to start: the Microphone key, a keyboard shortcut, or Edit, then Start Dictation ([Apple](${APPLE})).`,
        `Apple lists Fn-D as "start or stop dictation" and says you may be able to set a different shortcut in Keyboard settings ([Apple](${APPLE_KEYS})).`,
        `a shortcut that already belongs to another command or app won't work until one of them changes ([Apple](${APPLE_CUSTOM})).`,
      ],
    },
    { cta: { slot: "intro", button: "pick my own hold-to-talk key" } },

    { h: "what is the mac dictation shortcut?" },
    { p: `Apple lists Fn-D, but the answer depends on your Mac. Apple's shortcut list says "Fn-D: start or stop dictation" and adds that you may be able to set a different dictation shortcut in Keyboard settings ([Apple Support](${APPLE_KEYS})). the dictation guide points to the Microphone key "if available in the row of function keys" ([Apple Support](${APPLE})).` },
    { p: "so your shortcut depends on your keyboard and what you picked. the next section shows where it lives, and the [how to dictate on a mac guide](/blog/how-to-dictate-on-a-mac) covers the rest of the basics." },

    { h: "how do you set or change the dictation shortcut?" },
    { p: `open System Settings, click Keyboard in the sidebar, go to Dictation and switch it on (click Enable if it asks). then use the Shortcut pop-up to choose one. to make your own, choose Customize and press the keys you want. Apple's example is Option-Z ([Apple Support](${APPLE})).` },
    {
      ol: [
        "open the Apple menu, then System Settings, then Keyboard.",
        "go to Dictation and turn it on. click Enable if prompted.",
        "click the Shortcut pop-up and pick a listed shortcut.",
        "or choose Customize, then press the keys you want, such as Option-Z.",
        "click into a text field and try it.",
      ],
    },
    {
      cta: {
        slot: "mid",
        title: "want a key you only hold?",
        text: "SpeakOS lets you record any key, including a single modifier key held on its own. press it, talk, let go.",
        button: "set up my own key",
      },
    },

    { h: "how do you stop dictation?" },
    { p: `press Escape or the Microphone key ([Apple Support](${APPLE})). Apple's shortcut list also gives Fn-D as "start or stop dictation" ([Apple Support](${APPLE_KEYS})). dictation also stops by itself when it hears no speech for 30 seconds, though you can dictate text of any length otherwise. if it keeps quitting, our [dictation not working guide](/blog/mac-dictation-not-working) goes through every fix.` },
    { p: `one detail that trips people up: Apple says to press and release the Microphone key to start dictation, and to press and hold it to activate Siri if Siri is on ([Apple Support](${APPLE}), same wording in the [macOS 26 guide](${APPLE_26})). so holding that key opens Siri, not dictation. on a Mac with Apple silicon you can still use the keyboard while you speak, so you don't need to stop dictation to type a correction ([Apple Support](${APPLE_26})).` },

    { h: "how do you turn the dictation shortcut off?" },
    { p: `switch Dictation off. in System Settings, Keyboard, Dictation, turn it off ([Apple Support](${APPLE})). that is the only way Apple documents. Apple's own troubleshooting checklist starts with "be sure Dictation is turned on in Keyboard settings" ([Apple Support](${APPLE_FIX})), so expect the shortcut to do nothing until you switch Dictation back on from the same place.` },
    { p: "to free the key without turning Dictation off, pick another shortcut from the Shortcut pop-up or choose Customize ([Apple Support](${APPLE}))." },

    { h: "why is the mac dictation shortcut not working?" },
    { p: `most causes are on Apple's own checklist: Dictation is off, the language is wrong, or you're not using the right shortcut ([Apple Support](${APPLE_FIX})). the table below adds the shortcut-specific causes Apple documents.` },
    {
      table: {
        caption: "Why the dictation shortcut does nothing",
        head: ["symptom", "likely cause", "what Apple says"],
        rows: [
          ["no response at all", "Dictation is off, or you picked a different shortcut", `turn it on in Keyboard settings and check the Shortcut pop-up ([Apple](${APPLE}))`],
          ["dictation won't start, Voice Control is on", "Voice Control replaces Dictation", `"when Voice Control is on, you use Voice Control to dictate text; standard macOS Dictation isn't available" ([Apple](${APPLE_VC}))`],
          ["your custom shortcut is ignored", "it clashes with another shortcut", `a shortcut that already exists for another command or app won't work, so change one of them ([Apple](${APPLE_CUSTOM}))`],
          ["external keyboard, no Fn key", "no function key to press", `Apple says to hold Control while pressing a function key, and some non-Apple keyboards need a third-party utility ([Apple](${APPLE_FN}))`],
          ["Fn or Globe key behaves differently after you set a shortcut", "Keyboard settings changed along with the shortcut", `Apple's macOS 14 guide says that, depending on your Mac model, choosing a Dictation shortcut may automatically change the "Press fn key to" or "Press Globe key to" option in Keyboard settings ([Apple](${APPLE_14}))`],
        ],
      },
    },
    { p: "the Fn row comes from Apple's macOS 14 guide, so check your own Keyboard settings." },

    { h: "what can you say once dictation starts?" },
    { p: `quite a lot. Apple's commands page groups its commands under eight headings (punctuation, typography, formatting, capitalization, mathematical, currency, emoticons and intellectual property) and says the listed commands are always available while you dictate ([Apple Support](${APPLE_COMMANDS})). in supported languages, dictation also inserts commas, periods and question marks on its own, and you can turn that off with Auto-punctuation under Keyboard, Dictation ([Apple Support](${APPLE})).` },
    {
      table: {
        caption: "Commands Apple lists for dictating text",
        head: ["say", "result"],
        rows: [
          ["new line", "starts a new line"],
          ["new paragraph", "starts a new paragraph"],
          ["caps on / caps off", "Title Case for the next phrase, then back to normal"],
          ["all caps", "the next word in ALL CAPS"],
          ["no space on / no space off", "joins words without spaces, then resumes spacing"],
          ["numeral", "formats the next phrase as a number"],
          ["question mark, exclamation mark, comma, colon, semicolon", "the matching punctuation"],
          ["tab key", "moves the cursor to the next tab stop"],
        ],
      },
    },
    { p: `source: [Apple Support, commands for dictating text](${APPLE_COMMANDS}).` },

    { h: "is there a better shortcut than a toggle?" },
    { p: "a toggle asks you to press, talk, then press again, and Apple stops it after 30 seconds without speech. a held key flips that around. you decide when it starts and when it stops, and a pause costs you nothing." },
    { p: "SpeakOS is our Mac dictation app: hold a key, speak, and clean text lands in whatever app has focus. it's our product, so factor that in. the built-in shortcut is free and fine for a note. if you dictate all day, a key you hold may suit you better. the [dictation vs Voice Control guide](/blog/dictation-vs-voice-control-mac) explains where each Apple option fits. for writing mail, see how to [dictate emails on a mac](/blog/how-to-dictate-emails-on-mac), and [is dictation faster than typing?](/blog/is-dictation-faster-than-typing) covers the speed question." },
    { cta: { slot: "end", title: "choose your own key", text: "record the key you like, hold it, say the sentence. it lands wherever your cursor is.", button: "grab SpeakOS for my mac" } },

    {
      faq: [
        ["what is the keyboard shortcut for dictation on a mac?", `Apple lists Fn-D for start or stop dictation, and says you may be able to set a different one in Keyboard settings ([Apple Support](${APPLE_KEYS})). you can also use the Microphone key or Edit, then Start Dictation.`],
        ["how do you change the dictation shortcut on a mac?", `System Settings, Keyboard, Dictation, then the Shortcut pop-up. choose Customize to record your own keys, for example Option-Z ([Apple Support](${APPLE})).`],
        ["how do you stop dictation on a mac?", `press Escape, the Microphone key, or the dictation shortcut. it also stops after 30 seconds with no speech ([Apple Support](${APPLE})).`],
        ["why isn't my dictation shortcut working?", `check that Dictation is on, that Voice Control is off ([Apple Support](${APPLE_VC})), and that your shortcut doesn't clash with another one ([Apple Support](${APPLE_CUSTOM})). on a keyboard without an Fn key, Apple says to hold Control while pressing a function key.`],
        ["can you say punctuation when dictating on a mac?", `yes. say commands like "new line", "new paragraph", "question mark" or "comma". Apple's commands page lists them all ([Apple Support](${APPLE_COMMANDS})).`],
      ],
    },
  ],
};
