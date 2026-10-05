import { chartColors, gbar, hbar } from "../charts.ts";
import type { Post } from "../posts.ts";

const RUAN = "https://arxiv.org/abs/1608.07323";
const APPLE = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";

export const post: Post = {
  slug: "how-to-dictate-on-a-mac",
  title: "how do you dictate on a mac?",
  metaTitle: "How to Dictate on a Mac: 5 Easy Steps and Fixes",
  description: "Speech ran 2.93x faster than phone typing in a lab study (153 vs 52 wpm). Turn on Mac dictation in under a minute, see where it trips, and how to fix it.",
  date: "2026-10-05",
  checked: "2026-10-05",
  minutes: 7,
  tags: ["dictate on mac", "mac dictation", "voice typing mac"],
  cover: { src: "/blog/how-to-dictate-on-a-mac.webp", alt: "Pixel art of a small orange cat talking into a tin-can phone connected to a laptop, with speech turning into lines of text on the screen" },
  draft: false,
  blocks: [
    { p: `in a controlled study from Stanford, the University of Washington and Baidu, people spoke 153 words a minute into a phone and typed 52 ([Ruan et al.](${RUAN}), 2017). so why do most of us still type every email? mostly because dictation on a mac feels fiddly. you press a key, talk, pause to think, and it quits on you.` },
    { p: "this guide shows how to dictate on a mac using the dictation that's already built in: how to turn it on, where it trips people up, and what to change so you actually use it. we make SpeakOS, a mac app for hold-to-talk dictation, so we say where it fits and where it doesn't." },
    {
      takeaways: [
        `speech was 2.93x faster than phone typing in a lab study ([Ruan et al.](${RUAN}), 2017).`,
        `mac dictation is built in: System Settings, Keyboard, Dictation. it stops after 30 seconds of silence ([Apple](${APPLE})).`,
        "speech caught more errors during entry but left slightly more in the final text, so read it back.",
      ],
    },
    { cta: { slot: "intro", button: "skip the setup: try hold-to-talk on my mac" } },

    { h: "how do you turn on dictation on a mac?" },
    { p: `Apple puts Dictation in System Settings under Keyboard. you choose the shortcut there: the Microphone key, pressing Fn twice, or your own combination such as Option-Z ([Apple Support](${APPLE})). it takes a few clicks.` },
    {
      ol: [
        "open the Apple menu, then System Settings, then Keyboard.",
        "turn on Dictation.",
        "pick a shortcut from the pop-up, or choose Customize to set your own.",
        "click into any text field, press the shortcut and start talking.",
        "stop with Escape, the Microphone key, or the shortcut again.",
      ],
    },
    { p: "you can say punctuation and emoji names out loud, and words it isn't sure about get a blue underline. on a mac with Apple silicon you can keep typing while you talk. Apple says there's no need to stop dictation." },
    { p: "the shortcut is the part people fight with. if yours does nothing, the [mac dictation shortcut guide](/blog/mac-dictation-shortcut) walks through every fix." },
    {
      cta: {
        slot: "mid",
        title: "tired of the toggle?",
        text: "hold a key, say it, let go. SpeakOS types clean text into whatever app has focus, and nothing quits on you mid-sentence.",
        button: "get my mac typing for me",
      },
    },

    { h: "does mac dictation work offline?" },
    { p: `sometimes, and Apple says to check rather than assume. its guide tells you to look in Keyboard settings to see whether your voice input and transcripts are processed on your device and not sent to Siri servers ([Apple Support](${APPLE})). dictation also isn't available in every language or region.` },
    { p: "two limits catch people out. dictation stops after 30 seconds with no speech, so a long pause to think ends it. and standard Dictation isn't available while Voice Control is on, so if it won't start, check that first. our [dictation vs Voice Control comparison](/blog/dictation-vs-voice-control-mac) explains which one you want." },
    {
      table: {
        caption: "What Apple's dictation guide says, at a glance",
        head: ["question", "answer from Apple"],
        rows: [
          ["where do I turn it on?", "System Settings, Keyboard, Dictation"],
          ["what shortcuts can I use?", "the Microphone key, Fn twice, or your own via Customize"],
          ["how do I stop it?", "Escape, the Microphone key, or the shortcut"],
          ["does it have a time limit?", "no limit on length, but it stops after 30 seconds with no speech"],
          ["is it on-device?", "check Keyboard settings to see whether voice input and transcripts are processed on your device"],
          ["when is it unavailable?", "while Voice Control is on, and in some languages or regions"],
        ],
      },
    },
    { p: `source for every row: [Apple Support, Use Dictation on your Mac](${APPLE}), checked 5 October 2026.` },

    { h: "how much faster is talking than typing?" },
    { p: `about three times faster, in one lab study. people entered given phrases on an iPhone with speech input at 153 words a minute, against 52 on the built-in keyboard, a ratio of 2.93 ([Ruan et al.](${RUAN}), 2017). that's a phone test of copying phrases, not composing on a mac keyboard, but the gap is hard to ignore.` },
    {
      svg: hbar(
        "words per minute, speaking vs typing",
        "Bar chart. Speaking into a phone: 153 words per minute. Typing on a phone keyboard: 52. Average typist on a computer keyboard: 52, from a separate study.",
        [
          { label: "speaking (phone)", value: 153 },
          { label: "typing (phone)", value: 52, color: chartColors.navy },
          { label: "average typist (computer)", value: 52, color: "#9aa7bd" },
        ],
        "Sources: Ruan et al., 2017 (first two bars); Aalto University and Cambridge, 2018 (third bar).",
      ),
      caption: `the first two bars come from one study, run on phones. the grey bar is a different one: an average of 52 words a minute across 136 million keystrokes from 168,000 volunteers ([Aalto University](https://www.aalto.fi/en/news/the-traits-of-fast-typists-discovered-by-analysing-136-million-keystrokes), 2018). different people on different devices, so don't subtract one from the other.`,
    },
    { p: "the gap matters most for first drafts, where the slow part is getting the thought out, not polishing it. that's our read of the numbers, not something the studies measured. the full evidence is in [is dictation faster than typing?](/blog/is-dictation-faster-than-typing)" },

    { h: "does faster mean more accurate?" },
    { p: `not quite. in the same study, speech had fewer errors corrected during entry (5.30% against 11.22%) but left slightly more in the final text (1.30% against 0.79%) ([Ruan et al.](${RUAN}), 2017). the authors note it was a lab test and that further study is needed outside the laboratory. in practice, a dictated draft needs about the same proofreading as a typed one. treat the numbers as a direction for phone entry, not a promise for your mac.` },
    {
      svg: gbar(
        "errors: speech vs keyboard",
        "Grouped bar chart. Corrected during entry, speech had 5.30% errors and the keyboard 11.22%. Left in the final text, speech had 1.30% and the keyboard 0.79%.",
        ["corrected during entry", "left in final text"],
        [
          { name: "speech", color: chartColors.blue, values: [5.3, 1.3] },
          { name: "keyboard", color: chartColors.navy, values: [11.22, 0.79] },
        ],
        "Source: Ruan et al., 2017. Phone, lab setting.",
      ),
      caption: "a faster draft, the same proofreading job. the habit that makes dictation work is a quick read-back before you send.",
    },

    { h: "why does dictation invent words or miss names?" },
    { p: `speech models sometimes write text nobody said. a 2024 study of more than 13,000 clips found about 1% of Whisper transcriptions contained entire hallucinated phrases, more often around long pauses ([Cornell, FAccT 2024](https://www.news.cornell.edu/stories/2024/06/ai-speech-text-can-hallucinate-violent-language)). the clips came mostly from speakers with aphasia, alongside a comparison group, so everyday dictation should do better. silence is still where models slip.` },
    { note: "we hit this building SpeakOS. on near-silence, Whisper large-v3 confidently wrote stock outro lines like \"thanks for watching\". we now drop a known list of those phrases when the model also reports a high no-speech probability. the cutoff was tuned on about 500 samples across quiet and noisy rooms." },
    { p: `names are the other weak spot. in a [MacStories comparison](https://www.macstories.net/stories/hands-on-how-apples-new-speech-apis-outpace-whisper-for-lightning-fast-transcription/) of Apple's newer speech API and Whisper tools, quality was similar and all three tools stumbled on last names and unusual words (2025). so read back anything with a person or product name in it.` },

    { h: "when is hold-to-talk better than a toggle?" },
    { p: `when you pause to think. a toggle ends when you go quiet: Apple's dictation stops after 30 seconds of silence ([Apple Support](${APPLE})). hold-to-talk puts the boundary under your finger, so you hold a key, speak, and let go when you are done.` },
    { p: "SpeakOS works that way. hold the key, say it, and clean text lands in whatever app has focus: an email, a doc, a chat, a code comment. no window to switch to, nothing to paste. dictation runs on your mac by default and works offline, with a faster cloud option if you want it. the [privacy page](/privacy) has the details, and the [email guide](/blog/how-to-dictate-emails-on-mac) shows it in an inbox." },
    { p: "the built-in dictation is free and fine for a quick note. if you write all day, or you keep getting cut off mid-thought, try holding a key instead." },
    { ul: ["hold the key and say the whole message.", "read it back and fix what's off, especially names.", "send it."] },
    {
      cta: {
        slot: "end",
        title: "write your next email out loud",
        text: "hold the key, say the whole message, read it back. most of the time there is less to fix than you expect.",
        button: "grab SpeakOS for my mac",
      },
    },

    {
      faq: [
        ["is dictation on a mac free?", `yes. it's built into macOS. turn it on in System Settings, Keyboard, Dictation, then pick a shortcut ([Apple Support](${APPLE})).`],
        ["how do you stop dictation on a mac?", `press Escape, the Microphone key, or your dictation shortcut. it also stops by itself after 30 seconds with no speech ([Apple Support](${APPLE})).`],
        ["can you keep typing while you dictate?", `on a mac with Apple silicon, yes. Apple's guide says you can keep using the keyboard while you speak and there's no need to stop dictation ([Apple Support](${APPLE})).`],
        ["does mac dictation work without internet?", "it depends on your mac and language. Apple tells you to check Keyboard settings to see whether input is processed on your device. SpeakOS runs dictation on your mac by default and works offline."],
        ["is dictation faster than typing?", `in one lab study, speech input ran at 153 words a minute against 52 on a phone keyboard ([Ruan et al.](${RUAN}), 2017). it had slightly more uncorrected errors in the final text, so proofread.`],
      ],
    },
  ],
};
