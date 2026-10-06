import { blockHtml, type Block } from "./posts.ts";

// The about page doubles as the editorial policy the blog posts link to: who writes, how
// sources are chosen, how corrections work. Plain voice on purpose; the chrome around it follows
// the bro level, the policy does not.
export const ABOUT = {
  title: "about SpeakOS and how we write",
  metaTitle: "About SpeakOS and Our Editorial Policy",
  description: "SpeakOS is voice dictation and voice commands for the Mac. How we write the blog, which sources we trust, and how to send us a correction.",
};

const blocks: Block[] = [
  { p: "SpeakOS is voice dictation and voice commands for the Mac. Hold a key and speak, and clean text lands in whatever app has focus. Say what you want done and SpeakOS does it, showing each action before it happens." },
  { h: "who writes the blog" },
  { p: "the SpeakOS team writes it. we build the app, so we write about what we run into: dictation settings, speech recognition quirks, and the small habits that make talking to your mac work." },
  { h: "how we choose sources" },
  { ul: [
    "primary sources first: Apple, Google and Microsoft documentation for how their features work, and peer-reviewed papers for numbers.",
    "every statistic names its source, year and what was measured. a lab study on phones is called a phone study, not a mac one.",
    "if we can't open the page and see the number, we don't use it.",
    "we say when we sell something. SpeakOS is our product, and each post says so where it recommends it.",
  ] },
  { h: "what we don't claim" },
  { p: "we don't publish health, productivity or time-saving claims without a source we can read. when the evidence is thin, we say it's thin." },
  { h: "corrections" },
  { p: "found a mistake or a link that moved? tell us through the [bug report form](/bugs) and we'll fix it and note the change in the post's updated date." },
];

export const aboutHtml = () => blocks.map((b) => blockHtml(b, "about")).join("\n");
