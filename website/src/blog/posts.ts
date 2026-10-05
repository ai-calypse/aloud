import { chartColors, gbar, hbar } from "./charts.ts";

// Pure data, no React: the page renders it, and vite.config.ts bakes the same HTML into static
// files at build time so crawlers that never run JavaScript still read the post.
// Text may carry [label](url) links. Everything here is first-party, so it is escaped once and
// injected as HTML by both.
export type Block =
  | { h: string }
  | { p: string }
  | { ul: string[] }
  | { ol: string[] }
  | { takeaways: string[] }
  | { note: string }
  | { svg: string; caption: string }
  | { img: string; alt: string; caption?: string }
  | { faq: [string, string][] };

export type Post = {
  slug: string;
  title: string;
  description: string;
  date: string; // YYYY-MM-DD
  minutes: number;
  cover?: { src: string; alt: string }; // a file under public/, so the URL is stable for og:image
  // A draft is served but noindex and left out of the sitemap and the blog index.
  draft?: boolean;
  blocks: Block[];
};

const esc = (t: string) => t.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
const inline = (t: string) =>
  esc(t).replace(/\[([^\]]+)\]\(([^)\s]+)\)/g, (_m, label, href) => `<a href="${href}"${href.startsWith("http") ? ' rel="noopener" target="_blank"' : ""}>${label}</a>`);

export const blockHtml = (b: Block): string => {
  if ("h" in b) return `<h2>${esc(b.h)}</h2>`;
  if ("p" in b) return `<p>${inline(b.p)}</p>`;
  if ("ul" in b) return `<ul>${b.ul.map((li) => `<li>${inline(li)}</li>`).join("")}</ul>`;
  if ("ol" in b) return `<ol>${b.ol.map((li) => `<li>${inline(li)}</li>`).join("")}</ol>`;
  if ("takeaways" in b) return `<aside class="takeaways"><strong>key takeaways</strong><ul>${b.takeaways.map((li) => `<li>${inline(li)}</li>`).join("")}</ul></aside>`;
  if ("note" in b) return `<aside class="note">${inline(b.note)}</aside>`;
  if ("svg" in b) return `<figure>${b.svg}<figcaption>${inline(b.caption)}</figcaption></figure>`;
  if ("img" in b) return `<figure><img src="${b.img}" alt="${esc(b.alt)}" loading="lazy" />${b.caption ? `<figcaption>${inline(b.caption)}</figcaption>` : ""}</figure>`;
  return `<section class="faq"><h2>frequently asked questions</h2>${b.faq.map(([q, a]) => `<h3>${esc(q)}</h3><p>${inline(a)}</p>`).join("")}</section>`;
};
export const blocksHtml = (blocks: Block[]) => blocks.map(blockHtml).join("\n");

const RUAN = "https://arxiv.org/abs/1608.07323";
const APPLE = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";

export const POSTS: Post[] = [
  {
    slug: "how-to-dictate-on-a-mac",
    title: "how do you dictate on a mac? the built-in way, and a better one.",
    description: "Speech ran about 3x faster than phone typing in a lab study (153 vs 52 wpm). How to turn on dictation on a Mac, where it falls short, and when hold-to-talk fits better.",
    date: "2026-10-05",
    minutes: 6,
    draft: true,
    blocks: [
      { p: `in a controlled study from Stanford, the University of Washington and Baidu, people spoke 153 words a minute into a phone and typed 52 ([Ruan et al.](${RUAN}), 2017). so why do most of us still type every email? mostly because dictation on a mac feels fiddly. you press a key, talk, pause to think, and it quits on you.` },
      { p: "this guide shows how to turn on the dictation that's already on your mac, where it trips people up, and what to change so you actually use it." },
      {
        takeaways: [
          `speech was 2.93x faster than phone typing in a lab study ([Ruan et al.](${RUAN}), 2017).`,
          `mac dictation is built in: System Settings, Keyboard, Dictation. it stops after 30 seconds of silence ([Apple](${APPLE})).`,
          "speech made fewer errors while composing but left slightly more in the final text, so read it back.",
        ],
      },

      { h: "how do you turn on dictation on a mac?" },
      { p: `Apple puts Dictation in System Settings under Keyboard. you choose the shortcut there: the Microphone key, pressing Fn twice, or your own combination such as Option-Z ([Apple Support](${APPLE})). it takes under a minute.` },
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

      { h: "does mac dictation work offline?" },
      { p: `sometimes, and Apple says to check rather than assume. its guide tells you to look in Keyboard settings to see whether your voice input and transcripts are processed on your device and not sent to Siri servers ([Apple Support](${APPLE})). dictation also isn't available in every language or region.` },
      { p: "two limits catch people out. dictation stops after 30 seconds with no speech, so a long pause to think ends it. and standard Dictation isn't available while Voice Control is on, so if it won't start, check that first." },

      { h: "how much faster is talking than typing?" },
      { p: `about three times faster, in one lab study. 32 people composed texts on an iPhone with speech input at 153 words a minute, against 52 on the built-in keyboard, a ratio of 2.93 ([Ruan et al.](${RUAN}), 2017). that's a phone test, not a mac keyboard, but the gap is hard to ignore.` },
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
      { p: "the gap matters most for first drafts, where the slow part is getting the thought out, not polishing it. that's our read of the numbers, not something the studies measured." },

      { h: "does faster mean more accurate?" },
      { p: `not quite. in the same study, speech made fewer errors while people composed (5.30% against 11.22%) but left slightly more in the final text (1.30% against 0.79%) ([Ruan et al.](${RUAN}), 2017). the authors note it was a lab test and that real-world testing is still needed.` },
      {
        svg: gbar(
          "errors: speech vs keyboard",
          "Grouped bar chart. While composing, speech had 5.30% errors and the keyboard 11.22%. Left uncorrected in the final text, speech had 1.30% and the keyboard 0.79%.",
          ["while composing", "left in final text"],
          [
            { name: "speech", color: chartColors.blue, values: [5.3, 1.3] },
            { name: "keyboard", color: chartColors.navy, values: [11.22, 0.79] },
          ],
          "Source: Ruan et al., 2017. Phone, lab setting.",
        ),
        caption: "a faster draft, the same proofreading job. the habit that makes dictation work is a quick read-back before you send.",
      },

      { h: "why does dictation invent words or miss names?" },
      { p: `speech models sometimes write text nobody said. a 2024 study of more than 13,000 clips found about 1% of Whisper transcriptions contained entire hallucinated phrases, more often around long pauses ([Cornell, FAccT 2024](https://www.news.cornell.edu/stories/2024/06/ai-speech-text-can-hallucinate-violent-language)). the clips came from speakers with aphasia, so everyday dictation should do better. silence is still where models slip.` },
      { note: "we hit this building SpeakOS. on near-silence, Whisper large-v3 confidently wrote stock outro lines like \"thanks for watching\". we now drop a known list of those phrases when the model also reports a high no-speech probability. the cutoff was tuned on about 500 samples across quiet and noisy rooms." },
      { p: `names are the other weak spot. in a [MacStories comparison](https://www.macstories.net/stories/hands-on-how-apples-new-speech-apis-outpace-whisper-for-lightning-fast-transcription/) of Apple's newer speech API and Whisper tools, quality was similar and every tool stumbled on names (2025). so read back anything with a person or product name in it.` },

      { h: "when is hold-to-talk better than a toggle?" },
      { p: `when you pause to think. a toggle ends when you go quiet: Apple's dictation stops after 30 seconds of silence ([Apple Support](${APPLE})). hold-to-talk puts the boundary under your finger. you hold a key, speak, and let go.` },
      { p: "SpeakOS works that way. hold the key, say it, and clean text lands in whatever app has focus: an email, a doc, a chat, a code comment. no window to switch to, nothing to paste. dictation runs on your mac by default and works offline, with a faster cloud option if you want it. the [privacy page](/privacy) has the details." },
      { p: "the built-in dictation is free and fine for a quick note. if you write all day, or you keep getting cut off mid-thought, try holding a key instead." },
      { ul: ["hold the key and say the whole message.", "read it back and fix what's off, especially names.", "send it."] },

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
  },
];

export const publicPosts = () => POSTS.filter((p) => !p.draft);
export const postPath = (p: Post) => `/blog/${p.slug}`;
export const formatDate = (iso: string) => new Date(`${iso}T00:00:00Z`).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
