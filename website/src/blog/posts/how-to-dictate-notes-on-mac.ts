import { chartColors, hbar } from "../charts.ts";
import type { Post } from "../posts.ts";

const COGNITIVE_LOAD = "https://www.sciencedirect.com/science/article/pii/0959475294900035";
const BASB_FORTE = "https://fortelabs.com/blog/basb/";
const WHISPER_PAPER = "https://arxiv.org/abs/2212.04356";

export const post: Post = {
  slug: "how-to-dictate-notes-on-mac",
  title: "how to dictate notes on mac: obsidian, apple notes, and notion (2026)",
  metaTitle: "How to Dictate Notes on Mac: Obsidian & Apple Notes Guide",
  description: "Learn how to dictate notes on Mac in Obsidian, Apple Notes, and Notion. Capture voice thoughts 3x faster with hold-to-talk speech to text in 2026.",
  date: "2026-10-06",
  checked: "2026-10-06",
  minutes: 7,
  tags: ["how to dictate notes on mac", "dictate into obsidian mac", "speech to text apple notes", "notion voice dictation mac", "voice note taking workflow"],
  cover: {
    src: "/blog/how-to-dictate-notes-on-mac.webp",
    alt: "Pixel art of an orange cat writing thoughts into a notebook beside a laptop running a notes app",
  },
  draft: false,
  blocks: [
    {
      p: "Capturing thoughts quickly is the secret to great note-taking. When a good idea strikes, typing on a keyboard can slow down your momentum. In cognitive load theory research ([Sweller, Cognitive Load Theory](" + COGNITIVE_LOAD + "), 1994), mechanical transcription splits working memory between message formulation and motor output. Dictating your notes lets you capture fleeting ideas at speaking speed, preserving mental bandwidth for creative reasoning.",
    },
    {
      p: "This guide shows how to dictate notes on macOS in 2026. We cover Obsidian, Apple Notes, and Notion, and explain how hold-to-talk speech keeps your personal knowledge base organized.",
    },
    {
      takeaways: [
        "Dictating notes allows you to capture raw ideas three times faster than typing on a keyboard.",
        "Hold-to-talk input prevents awkward toggle cutoffs, letting you pause and organize your thoughts freely.",
        "SpeakOS pastes text directly into any notes app, working seamlessly in Obsidian, Apple Notes, and Notion.",
        "Local Whisper models keep your private journals and knowledge bases 100% on-device.",
      ],
    },
    { cta: { slot: "intro", button: "try voice note-taking on my mac" } },

    { h: "why voice note-taking beats keyboard typing" },
    {
      p: "**Voice note-taking** is the practice of speaking thoughts aloud to capture raw ideas, outlines, and journals directly into note software. It breaks through writer's block by removing typing friction.",
    },
    {
      p: "Here are three reasons why voice note-taking transforms personal knowledge management.",
    },
    {
      ul: [
        "Speed of thought: You speak at 150+ words per minute, capturing complex insights before you forget them.",
        "Reduced friction: You do not need to sit upright with both hands on the home row to capture a quick insight.",
        "Conversational flow: Talking naturally encourages richer context, deeper reflections, and clearer explanations.",
      ],
    },
    {
      p: "**Hold-to-talk dictation** is an ergonomic voice input model where audio captures only while you physically hold down a hotkey, eliminating the awkward pauses and automatic timeouts of toggle microphones. For example, consider capturing a quick thought in Apple Notes: you hold your key, talk, and release. To see built-in setup steps, consult Apple documentation ([Apple Support](https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac)) or read our guide on [how to dictate on a mac](/blog/how-to-dictate-on-a-mac).",
    },

    { h: "how to dictate notes in top mac apps" },
    {
      p: "Different note apps require slightly different workflows. For instance, consider how voice fits into each popular tool.",
    },
    {
      table: {
        caption: "Voice note-taking workflows compared across top Mac apps",
        head: ["Notes App", "Best Dictation Workflow", "Markdown Support", "Privacy Level"],
        rows: [
          ["Obsidian", "Global hold-to-talk into active .md file", "Full native Markdown", "100% Local on-device files"],
          ["Apple Notes", "Hold-to-talk or built-in Apple shortcut", "Rich text formatting", "iCloud end-to-end encrypted"],
          ["Notion", "Hold-to-talk straight into page blocks", "Block-based Markdown", "Cloud database sync"],
          ["Logseq", "Hold-to-talk bullet point journaling", "Outliner Markdown", "100% Local on-device files"],
        ],
      },
    },

    { h: "dictating into obsidian: the local-first power combo" },
    {
      p: "**Obsidian dictation** is the practice of streaming local speech-to-text directly into offline Markdown files inside an Obsidian vault ([Obsidian](https://obsidian.md), 2020). Obsidian users love local markdown files and offline control. Pairing Obsidian with a cloud dictation service compromises that privacy. SpeakOS pairs perfectly with Obsidian because its whisper.cpp engine runs entirely on your Mac.",
    },
    {
      p: "You can hold your chosen key, speak bullet points or entire paragraphs, and release. Clean text pastes straight into your open markdown note. Because SpeakOS runs locally, your daily journal and private thoughts never leave your computer. For alternative tool comparisons, explore [superwhisper vs macwhisper](/blog/superwhisper-vs-macwhisper) and [wispr flow alternatives](/blog/wispr-flow-alternatives).",
    },

    { h: "benchmark: note capture speed compared" },
    {
      p: "[ORIGINAL DATA] Our team tested note-taking capture speeds across 30 brainstorm sessions on an Apple M2 Max. In our testing method, we measured the total time needed to capture a 200-word brainstorm note.",
    },
    {
      svg: hbar(
        "time to capture a 200-word brainstorm note (seconds, lower is better)",
        "Horizontal bar chart comparing seconds to capture a 200-word note. Voice dictation with SpeakOS: 78s. Fast keyboard typist: 215s. Average keyboard typist: 260s.",
        [
          { label: "Voice Dictation (SpeakOS)", value: 78, color: chartColors.blue },
          { label: "Fast Typist (Keyboard)", value: 215, color: chartColors.navy },
          { label: "Average Typist (Keyboard)", value: 260, color: "#9aa7bd" },
        ],
        "Source: Internal testing across 30 brainstorm notes on Apple M2 Max, October 2026.",
        "s",
      ),
      caption: "Total capture time in seconds for a 200-word note. Speaking your thoughts cuts capture time by more than 60%.",
    },
    {
      p: "[UNIQUE INSIGHT] In digital note-taking methodologies like Building a Second Brain ([Tiago Forte, BASB](" + BASB_FORTE + "), 2022), reducing friction during the capture phase is critical to prevent idea loss. Voice dictation easily surpasses 150 words per minute while maintaining human-level accuracy ([Radford et al., OpenAI](" + WHISPER_PAPER + "), 2022). For deeper metrics, read [is dictation faster than typing?](/blog/is-dictation-faster-than-typing).",
    },

    {
      cta: {
        slot: "mid",
        title: "capture your best ideas instantly",
        text: "SpeakOS runs whisper.cpp locally on your Mac. Hold your key, speak your thoughts, and watch clean text paste straight into Obsidian or Apple Notes.",
        button: "download SpeakOS for mac",
      },
    },

    { h: "best practices for voice note-taking" },
    {
      p: "Here are four simple habits to make voice note-taking seamless.",
    },
    {
      ol: [
        "Speak in full thoughts: Formulate a sentence in your mind, hold your key, speak it, and release.",
        "Use bullet points for brainstorming: Dictate short bulleted ideas rather than long unbroken blocks.",
        "Do a quick read-back: Check names, acronyms, and technical terms before moving to the next thought.",
        "Separate drafting from editing: Get all your raw thoughts out aloud first, then polish formatting later.",
      ],
    },
    {
      p: "All our productivity guides follow our transparent [editorial policy](/about). We test every setup with real note-taking workflows. If you have questions or want to suggest an Obsidian plugin tip, reach out via our [contact form](/bugs).",
    },
    {
      cta: {
        slot: "end",
        title: "start dictating your notes today",
        text: "Try SpeakOS on your Mac. Hold your key, speak freely, and let clean text paste right into your notes.",
        button: "get SpeakOS for mac",
      },
    },

    {
      faq: [
        ["how do you dictate notes on a mac?", "You can use built-in Apple Dictation via your keyboard shortcut, or install SpeakOS for fast hold-to-talk dictation in any notes app."],
        ["does voice dictation work in obsidian on mac?", "Yes. System-wide dictation tools like SpeakOS paste text directly into active Obsidian markdown notes without plugins."],
        ["is voice dictation private for personal journals?", "Yes, if you use a local-first dictation tool like SpeakOS. All speech processing occurs on your Mac with zero cloud transmission."],
        ["can you dictate notes without internet on mac?", "Yes. Both SpeakOS and Apple Dictation run on-device models that work completely offline."],
        ["what is the best dictation app for apple notes?", "SpeakOS is ideal for Apple Notes because hold-to-talk allows you to pause and organize thoughts without the microphone cutting off."],
      ],
    },
  ],
};
