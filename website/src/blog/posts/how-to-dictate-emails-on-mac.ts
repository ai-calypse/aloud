import type { Post } from "../posts.ts";

const APPLE = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";
const OUTLOOK = "https://support.microsoft.com/en-us/office/dictate-your-emails-in-outlook-4010d238-bb25-45e9-89f6-8f9b54fcc0fc";
const DOCS = "https://support.google.com/docs/answer/4492226?hl=en";
const GMAIL_A11Y = "https://support.google.com/accessibility/answer/6115187?hl=en&co=GENIE.Platform%3DDesktop";
const GBOARD = "https://support.google.com/gboard/answer/2781851?hl=en&co=GENIE.Platform%3DAndroid";

export const post: Post = {
  slug: "how-to-dictate-emails-on-mac",
  title: "how to dictate emails on a mac: Mail, Gmail and Outlook.",
  metaTitle: "How to Dictate Emails on a Mac: Mail, Gmail, Outlook [2026]",
  description: "macOS Dictation works in any text field and stops after 30 seconds of silence. Steps to dictate emails in Apple Mail, Gmail and Outlook for Mac, plus fixes.",
  date: "2026-10-05",
  checked: "2026-10-05",
  minutes: 6,
  tags: ["how to dictate emails", "dictate emails in gmail", "dictate emails in outlook mac"],
  cover: { src: "/blog/how-to-dictate-emails-on-mac.webp", alt: "Pixel art of an orange cat talking into a tin-can phone, with speech bubbles drifting toward an envelope on the floor" },
  blocks: [
    { p: `"with Dictation, you can speak to enter text anywhere you can type it" ([Apple Support](${APPLE})). that includes the compose window of every email app you use. so the real question isn't whether you can dictate emails on a mac. it's which of three routes suits your inbox.` },
    { p: "the routes are Apple Mail with macOS dictation, Gmail in a browser, and Outlook for Mac with its own Dictate button. they work differently, they need different things, and one of them needs a paid subscription. here's each one, side by side." },
    {
      takeaways: [
        `macOS Dictation works in any compose box, and stops after 30 seconds without speech ([Apple](${APPLE})).`,
        `Outlook for Mac has its own Dictate button, but needs Microsoft 365 and a reliable internet connection ([Microsoft](${OUTLOOK})).`,
        "we found no Google documentation of voice typing inside Gmail's compose window on desktop, so use macOS Dictation there.",
      ],
    },
    { cta: { slot: "intro", button: "dictate my emails with one held key" } },

    { h: "how do you dictate an email in Apple Mail?" },
    { p: `turn on Dictation, then start it from the compose window (the full setup is in [how to dictate on a mac](/blog/how-to-dictate-on-a-mac)). Apple lists three ways to start: the Microphone key, your Dictation keyboard shortcut, or Edit, then Start Dictation ([Apple Support](${APPLE})). to switch it on, open System Settings, click Keyboard, go to Dictation and turn it on.` },
    {
      ol: [
        "open Mail and start a new message or reply.",
        "click into the body of the email.",
        "choose Edit, then Start Dictation, or press your shortcut or the Microphone key.",
        "speak. say the punctuation, for example \"exclamation mark\", \"new line\" or \"new paragraph\" ([Apple's command list](https://support.apple.com/guide/mac-help/commands-for-dictating-text-on-mac-mh40695/mac)).",
        "press Escape, the Microphone key or the shortcut to stop.",
      ],
    },
    { p: `in supported languages, dictation also adds commas, periods and question marks on its own ([Apple Support](${APPLE})). it isn't available in every language or region. if it won't start, the [mac dictation not working guide](/blog/mac-dictation-not-working) has Apple's checklist, and the [shortcut guide](/blog/mac-dictation-shortcut) covers the keys.` },

    { h: "how do you dictate emails in Gmail on a mac?" },
    { p: `use macOS Dictation in the compose box. Gmail's compose window is a text field, and Apple says Dictation works anywhere you can type text ([Apple Support](${APPLE})). click into the message, press your dictation shortcut, and talk.` },
    { p: `we looked for a Gmail-specific option and didn't find one, and Google's Gmail accessibility page lists only a generic "voice input" platform feature ([Google Help](${GMAIL_A11Y})). Google Docs has its own voice typing, but it works in Docs and Slides, not in Gmail ([Google Docs Help](${DOCS})). the Gboard keyboard names Gmail, but that's for phones ([Gboard Help](${GBOARD})).` },
    {
      cta: {
        slot: "mid",
        title: "same habit, any inbox",
        text: "SpeakOS works in Mail, Gmail and Outlook the same way: hold a key and the text lands in whichever compose box has focus.",
        button: "get my mac typing for me",
      },
    },

    { h: "how do you dictate emails in Outlook for Mac?" },
    { p: `Outlook for Mac has a built-in Dictate button. start a new email or reply, then go to Message, then Dictate, or press Option + F1. wait for the button to turn on and start listening ([Microsoft Support](${OUTLOOK})).` },
    { p: `there are conditions. Microsoft says dictation in Outlook uses a microphone and a reliable internet connection, and it's for Microsoft 365 subscribers ([Microsoft Support](${OUTLOOK})). you can set the spoken language, the microphone and auto punctuation in its settings. Microsoft says the service doesn't store your audio or transcribed text, but your speech is sent to Microsoft to produce the text ([Microsoft Support](${OUTLOOK})).` },
    { p: `its spoken commands include period, comma, question mark, exclamation point, new line, colon and semicolon, plus open and close quotes, parentheses, hyphen and ellipsis ([Microsoft Support](${OUTLOOK})). Microsoft's English command list doesn't include "new paragraph", so say "new line" twice instead of assuming it works.` },

    { h: "which method should you use?" },
    {
      table: {
        caption: "Three ways to dictate an email on a Mac",
        head: ["", "Apple Mail", "Gmail in a browser", "Outlook for Mac"],
        rows: [
          ["how you start", "Edit, Start Dictation, Microphone key or your shortcut", "macOS Dictation shortcut in the compose box", "Message, then Dictate, or Option + F1"],
          ["what it costs", "free, built into macOS", "free, built into macOS", "needs a Microsoft 365 subscription"],
          ["internet", "check Keyboard settings for on-device processing", "check Keyboard settings for on-device processing", "reliable connection required"],
          ["how to stop", "Escape, Microphone key or shortcut", "Escape, Microphone key or shortcut", "not documented by Microsoft"],
          ["privacy note", "Keyboard settings show whether input goes to Siri servers", "Keyboard settings show whether input goes to Siri servers", "Microsoft says audio and text aren't stored, but speech is sent to Microsoft"],
        ],
      },
    },
    { p: `use macOS Dictation for Mail and Gmail, and Outlook's Dictate button only if you already pay for Microsoft 365. sources: [Apple Support](${APPLE}) and [Microsoft Support](${OUTLOOK}). the Gmail column reflects Apple's "anywhere you can type" rule, since Google documents no Gmail-specific voice typing on desktop.` },
    { img: "/blog/cat-glasses-cat.webp", alt: "A cat in round glasses and a red bow tie working at a laptop", caption: "proofreading, but make it fashionable.", width: 520, height: 510 },

    { h: "how do you stop dictated emails from going wrong?" },
    { p: `read before you send, because speech recognition gets proper nouns wrong. Apple underlines words it isn't sure about in blue ([Apple Support](${APPLE})), and in one comparison of Apple's speech tools and Whisper, all three tools stumbled on last names and unusual words ([MacStories](https://www.macstories.net/stories/hands-on-how-apples-new-speech-apis-outpace-whisper-for-lightning-fast-transcription/), 2025).` },
    {
      ul: [
        "check names, email addresses and numbers first. they're what people notice.",
        "say punctuation and line breaks out loud as you go, so you don't reformat at the end.",
        "dictate one thought at a time. Apple says there's no length limit, but dictation stops after 30 seconds without speech, so a long pause ends it.",
        "skim the greeting and sign-off. they're easy to mangle.",
      ],
    },
    { p: "if you want the research behind the read-back habit, [is dictation faster than typing?](/blog/is-dictation-faster-than-typing) covers the error rates." },

    { h: "what changes with hold-to-talk?" },
    { p: "you stop managing a toggle. you hold a key, say the paragraph, and let go, so a pause to find the right word doesn't end anything. SpeakOS is our Mac dictation app, and it pastes clean text into whichever app has focus, so the same key works in Mail, a Gmail tab and Outlook. it's our product, so weigh that. the built-in routes above are free and good enough for the odd email. if your inbox is the job, try holding a key for a week, and see [dictation vs voice control on mac](/blog/dictation-vs-voice-control-mac) if you also want to run the inbox by voice." },
    { note: "last checked against Apple's and Microsoft's support pages on 5 October 2026. spot an error? [tell us](/bugs). how we write and check posts: [editorial policy](/about)." },
    { cta: { slot: "end", title: "answer your inbox out loud", text: "hold the key, say the reply, read it back, send.", button: "grab SpeakOS for my mac" } },

    {
      faq: [
        ["how do you dictate emails on a mac?", `turn on Dictation in System Settings, Keyboard, then click into the email and press the Microphone key, your shortcut, or Edit, then Start Dictation ([Apple Support](${APPLE})).`],
        ["how do you dictate emails in Gmail?", `use macOS Dictation in the compose box, since Apple says it works anywhere you can type ([Apple Support](${APPLE})). we found no built-in Gmail voice typing for desktop in Google's help pages.`],
        ["how do you dictate emails in Outlook for Mac?", `start a new email, choose Message, then Dictate, or press Option + F1 ([Microsoft Support](${OUTLOOK})). it needs a Microsoft 365 subscription and a reliable internet connection.`],
        ["does Outlook dictation need internet?", `yes. Microsoft says dictation in Outlook uses a microphone and a reliable internet connection ([Microsoft Support](${OUTLOOK})). macOS Dictation may work on-device, so check your Keyboard settings.`],
        ["how do you stop dictation in an email?", `in Mail or Gmail, press Escape, the Microphone key or your shortcut. it also stops after 30 seconds with no speech ([Apple Support](${APPLE})). Microsoft's Outlook page doesn't document how to stop it.`],
      ],
    },
  ],
};
