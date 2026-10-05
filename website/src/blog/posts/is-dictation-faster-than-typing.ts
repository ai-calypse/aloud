import { chartColors, gbar, hbar } from "../charts.ts";
import type { Post } from "../posts.ts";

const RUAN = "https://arxiv.org/abs/1608.07323";
const AALTO = "https://www.aalto.fi/en/news/the-traits-of-fast-typists-discovered-by-analysing-136-million-keystrokes";
const LIBERMAN = "https://languagelog.ldc.upenn.edu/~myl/languagelog/archives/003423.html";
const GOULD_BOIES = "https://research.ibm.com/publications/writing-dictating-and-speaking-letters";
const GOULD = "https://research.ibm.com/publications/how-experts-dictate";
const HODGSON = "https://academic.oup.com/jamia/article/24/6/1127/4049461";
const REVIEW = "https://pmc.ncbi.nlm.nih.gov/articles/PMC4954615";
const DELAPAZ = "https://asu.elsevierpure.com/en/publications/effects-of-dictation-and-advanced-planning-instruction-on-the-com";
const BLACKLEY = "https://pubmed.ncbi.nlm.nih.gov/32521449/";

export const post: Post = {
  slug: "is-dictation-faster-than-typing",
  title: "is dictation faster than typing? what the research shows.",
  metaTitle: "Is Dictation Faster Than Typing? What Research Shows (2026)",
  description: "Speech input hit 153 words a minute against 52 on a phone keyboard, but copying phrases isn't composing. What the studies say on speed, errors and quality.",
  date: "2026-10-05",
  checked: "2026-10-05",
  minutes: 8,
  tags: ["dictation vs typing", "is dictation faster than typing", "typing vs dictation speed"],
  cover: { src: "/blog/is-dictation-faster-than-typing.webp", alt: "Pixel art race track with two lanes: a long blue bar with a microphone for speaking, and a short pale bar with a keyboard for typing, and a yellow finish flag" },
  blocks: [
    { p: `speech input beat typing 153 words a minute to 52 in a controlled study, a ratio of 2.93 ([Ruan et al.](${RUAN}), 2017). it's a number that "dictation is 3x faster" headlines tend to lean on. it's also a number about copying phrases on a phone, which is a different job from writing an email.` },
    { p: "so is dictation faster than typing? it depends what you're doing. here's what the research does and doesn't show, with the limits of each study stated next to its result." },
    {
      takeaways: [
        `speech input ran at 153 words a minute against 52 for a phone keyboard, but participants entered given phrases ([Ruan et al.](${RUAN}), 2017).`,
        `for composing, planning takes about two-thirds of the time whether you write or dictate ([Gould and Boies](${GOULD_BOIES}), 1978).`,
        "dictation produced longer, higher-rated notes in one small study, while an emergency-department software test found it 18% slower with more errors.",
      ],
    },
    { cta: { slot: "intro", button: "test it yourself: put SpeakOS on my mac" } },

    { h: "is dictation faster than typing?" },
    { p: `for entering text you already have in mind, yes, by a wide margin. in the Stanford, University of Washington and Baidu study, speech input was 2.93 times faster than the built-in iPhone keyboard in English (153 against 52 words a minute), and 2.87 times faster in Mandarin ([Ruan et al.](${RUAN}), 2017).` },
    { p: "the catch is in the method. participants entered given phrases on an iPhone 6 Plus in a quiet indoor setting, so it measured transcription speed, not how fast people compose." },

    { h: "how fast do people actually type and talk?" },
    { p: `the average typist manages 52 words a minute. an Aalto University and Cambridge study, presented at CHI 2018, of 136 million keystrokes from 168,000 volunteers found most people type between 30 and 60, with the fastest reaching 120 ([Aalto University](${AALTO}), 2018). trained typists studied decades earlier typically reached 60 to 90.` },
    { p: `speaking is faster than either. in casual English telephone conversation, researchers measured roughly 164 words a minute per speaking turn and 196 overall, and 236 once silences are removed ([Language Log](${LIBERMAN}), quoting Yuan, Liberman and Cieri, 2006). the figures shift with how you count, so read them as "about 165 to 235".` },
    {
      svg: hbar(
        "words per minute, by task",
        "Bar chart of words per minute. Average typist, copy-typing test on a computer: 52. Typing on a phone keyboard in a lab: 52. Speech input on a phone in a lab: 153. Casual conversation, per speaking turn: 164.",
        [
          { label: "average typist (computer)", value: 52, color: "#9aa7bd" },
          { label: "typing (phone, lab)", value: 52, color: chartColors.navy },
          { label: "speech input (phone, lab)", value: 153 },
          { label: "casual talk (per turn)", value: 164, color: "#7fb0ff" },
        ],
        "Sources: Aalto/Cambridge 2018; Ruan et al. 2017; Yuan, Liberman and Cieri 2006.",
      ),
      caption: "four different tasks, so don't read the chart as one league table. conversation is casual talk, not composed writing, and it likely overstates dictation because people pause to think when they compose.",
    },
    { img: "/blog/cat-time-cat.webp", alt: "A cream cat looking at the camera while two pointing-hand emoji tap a wristwatch", caption: "the clock is running while you think, however you write.", width: 720, height: 857 },

    { h: "does dictation make you faster at composing?" },
    { p: `not necessarily, and the oldest evidence says why. in 1978 researchers found that planning takes about two-thirds of composition time whichever method you use, and that novices could learn within hours to dictate with the speed and quality with which they write ([Gould and Boies](${GOULD_BOIES}), 1978). thinking is the bottleneck, not your fingers.` },
    { p: `speed gains for experienced dictators were real but bounded: executives were 20 to 65 percent faster dictating than writing, and the author concluded that "composition seems to be the fundamental skill, and method of composition is of secondary importance" ([Gould](${GOULD}), 1978). those were human dictation setups, not speech-to-text.` },
    {
      table: {
        caption: "What studies found when people composed or documented with dictation",
        head: ["study", "who and what", "result", "limit"],
        rows: [
          [`[Gould and Boies, 1978](${GOULD_BOIES})`, "writing vs dictating letters", "planning about two-thirds of the time; novices matched their writing speed and quality", "human dictation, abstract only"],
          [`[Blackley et al., 2020](${BLACKLEY})`, "10 physicians, simulated visits", "dictated notes longer (320.6 vs 180.8 words) and rated higher (7.7 vs 6.6); time comparable; typed notes had more uncorrected errors (2.9 vs 1.5)", "very small sample"],
          [`[Hodgson et al., 2017](${HODGSON})`, "35 clinicians, record tasks", "speech 18.11% slower overall (165.46 s vs 140.09 s), 138 errors vs 32", "emergency department, 2017-era software; authors blame part of the error rise on EHR integration"],
          [`[De La Paz and Graham, 1997](${DELAPAZ})`, "students with learning disabilities", "better essays when dictation came with planning instruction", "grades 5 to 7, not adult writers"],
        ],
      },
    },
    {
      cta: {
        slot: "mid",
        title: "plan out loud, then talk it through",
        text: "the research says planning is the slow part. SpeakOS lets you think in a sentence, hold a key and say the draft, so the planning happens at the speed you talk.",
        button: "get my mac typing for me",
      },
    },

    { h: "is dictated text more accurate?" },
    { p: `speech made fewer errors while people were entering text, but left slightly more in the final version. in the phone study, 5.30% of speech entries were corrected during entry against 11.22% for the keyboard, while 1.30% of speech text stayed wrong against 0.79% ([Ruan et al.](${RUAN}), 2017). fast and clean are different claims.` },
    {
      svg: gbar(
        "errors: speech vs keyboard",
        "Grouped bar chart. Corrected during entry, speech 5.30% and keyboard 11.22%. Left in the final text, speech 1.30% and keyboard 0.79%.",
        ["corrected during entry", "left in final text"],
        [
          { name: "speech", color: chartColors.blue, values: [5.3, 1.3] },
          { name: "keyboard", color: chartColors.navy, values: [11.22, 0.79] },
        ],
        "Source: Ruan et al., 2017. Phone, lab setting.",
      ),
      caption: "speech caught more errors on the way in and let slightly more slip through. proofreading is where the accuracy comes back.",
    },
    { p: `clinical settings are less clear-cut. a 2016 systematic review of 23 studies called the evidence for speech recognition in clinical documentation "weak", though turnaround improved in every study that measured it, mostly against human transcription services, not against typing ([Hodgson and Coiera](${REVIEW}), 2016).` },

    { h: "does dictating change the quality of what you write?" },
    { p: `possibly, in the right conditions. in the small physician study, dictated notes were longer and scored 7.7 against 6.6 on quality ([Blackley et al.](${BLACKLEY}), 2020). in a school study, dictation produced better essays only when it came with advanced-planning instruction ([De La Paz and Graham](${DELAPAZ}), 1997).` },
    { p: "both results point the same way. dictation works best when you've planned what you'll say. we found no modern peer-reviewed study showing that speaking while composing makes thinking better or worse, so we won't claim either." },

    { h: "what doesn't the evidence show?" },
    { p: "five claims you'll see repeated that these studies do not support:" },
    {
      ul: [
        "that dictation is three times faster for composing. that figure comes from copying phrases on a phone.",
        "that dictated text is more accurate overall, or less. uncorrected errors were higher with speech in the phone study and lower in the physician study.",
        "a modern words-per-minute figure for dictating prose on a desktop. we found no peer-reviewed number.",
        "any benefit for RSI or fatigue. we found no source we could trust, so we make no claim.",
        "how today's AI dictation tools perform. every study above predates them. for what the built-in mac tool does and where it breaks, see [mac dictation not working](/blog/mac-dictation-not-working).",
      ],
    },

    { h: "how do you get the speed without the errors?" },
    { p: "use the evidence. plan the point in a sentence first, since planning is the slow part. then say the draft in one go with a [dictation shortcut](/blog/mac-dictation-shortcut) you can hold without thinking, and read it back once for names and numbers. that's the whole method." },
    { p: "if you want to try it on a mac, the [how to dictate on a mac guide](/blog/how-to-dictate-on-a-mac) covers the built-in option, and the [email guide](/blog/how-to-dictate-emails-on-mac) shows the habit in an inbox. SpeakOS is our product, a voice dictation app for the mac, so weigh our enthusiasm accordingly." },
    { img: "/blog/cat-write-cat.webp", alt: "A tabby cat typing on a laptop at a desk", caption: "the control group.", width: 390, height: 520 },
    { cta: { slot: "end", title: "run your own test", text: "write your next email twice, once typed and once spoken, and time both. it takes five minutes.", button: "try it on my mac" } },

    {
      faq: [
        ["is dictation faster than typing?", `for entering known text, yes: 153 against 52 words a minute in a phone study ([Ruan et al.](${RUAN}), 2017). for composing, the evidence is mixed because planning dominates the time ([Gould and Boies](${GOULD_BOIES}), 1978).`],
        ["how many words per minute can you dictate?", `casual English conversation runs about 165 to 235 words a minute depending on how you count ([Language Log](${LIBERMAN}), 2006). dictating composed text is likely slower because you pause to think.`],
        ["is dictation more accurate than typing?", `the evidence conflicts. speech left 1.30% errors in the final text against 0.79% for the keyboard in one phone study ([Ruan et al.](${RUAN}), 2017), and an emergency-department test counted 138 errors for speech against 32 for keyboard and mouse ([Hodgson et al.](${HODGSON}), 2017). but in a 10-physician study, typed notes had more uncorrected errors than dictated ones, 2.9 against 1.5 ([Blackley et al.](${BLACKLEY}), 2020).`],
        ["what is the average typing speed?", `52 words a minute, from 136 million keystrokes by 168,000 volunteers ([Aalto University](${AALTO}), 2018). most people type between 30 and 60.`],
        ["does dictation improve writing quality?", `in one small study of 10 physicians, dictated notes were longer and rated 7.7 against 6.6 ([Blackley et al.](${BLACKLEY}), 2020). it's early evidence, not a rule.`],
      ],
    },
  ],
};
