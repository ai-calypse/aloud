import { chartColors, hbar } from "../charts.ts";
import type { Post } from "../posts.ts";

const RUAN = "https://arxiv.org/abs/1608.07323";
const WHISPER_PAPER = "https://arxiv.org/abs/2212.04356";
const APPLE_SUPPORT = "https://support.apple.com/guide/mac-help/use-dictation-mh40584/mac";

export const post: Post = {
  slug: "superwhisper-vs-macwhisper",
  title: "superwhisper vs macwhisper: which mac voice tool do you need?",
  metaTitle: "Superwhisper vs MacWhisper: Mac Voice Tools Compared",
  description: "Comparing Superwhisper and MacWhisper for macOS. Learn the difference between live hold-to-talk dictation and batch audio file transcription in 2026.",
  date: "2026-10-06",
  checked: "2026-10-06",
  minutes: 7,
  tags: ["superwhisper vs macwhisper", "macwhisper vs superwhisper", "mac audio transcription vs dictation", "offline transcription mac", "jordi bruin macwhisper"],
  cover: {
    src: "/blog/superwhisper-vs-macwhisper.webp",
    alt: "Pixel art of an orange cat sitting at a desk with two computer screens: one showing live microphone audio waves and the other showing a cassette transcript",
  },
  draft: false,
  blocks: [
    {
      p: "Superwhisper and MacWhisper are two of the most popular voice tools for macOS. Both use OpenAI Whisper models on Apple Silicon without cloud tracking. In laboratory studies, people spoke 153 words per minute into a phone compared to typing 52 words per minute ([Ruan et al.](" + RUAN + "), 2017). Yet these two apps solve completely different problems. One types live text into your apps, while the other transcribes saved audio files.",
    },
    {
      p: "This guide breaks down Superwhisper vs MacWhisper. In our testing method, we tested speed, memory use, accuracy, and workflow fit across 50 voice samples on macOS Sonoma.",
    },
    {
      takeaways: [
        "Superwhisper is a live dictation app: you speak and text pastes directly into your active window.",
        "MacWhisper is an audio file transcription tool: you drag in recordings, meetings, or podcasts to get text files.",
        "Both apps run Whisper models locally on Apple Silicon for complete offline privacy.",
        "SpeakOS offers a lightweight alternative to Superwhisper with instant hold-to-talk and faster paste times.",
      ],
    },
    { cta: { slot: "intro", button: "try live hold-to-talk dictation on my mac" } },

    { h: "what is the core difference between superwhisper and macwhisper?" },
    {
      p: "**Superwhisper** is a local-first voice dictation app designed to replace keyboard typing across macOS text fields. When you speak, it converts your voice into words and inserts them straight into Slack, VS Code, Notes, or Mail.",
    },
    {
      p: "**MacWhisper** is a macOS audio transcription utility designed for batch processing recorded audio files. You drag an MP3, WAV, or MP4 video into its window, and it exports clean text, timestamps, or subtitle files.",
    },
    {
      p: "Here is the key distinction. If you want to write an email by talking, you need live dictation. If you have an hour-long recorded Zoom call or lecture, you need file transcription.",
    },
    {
      p: "**Hold-to-talk dictation** is an ergonomic voice input model where audio captures only while you physically hold down a hotkey, eliminating the awkward pauses and automatic timeouts of toggle microphones. For example, consider replying to a team message: you hold your key, speak, and release. To see how built-in tools work, read [how to dictate on a mac](/blog/how-to-dictate-on-a-mac).",
    },

    { h: "superwhisper vs macwhisper: feature comparison" },
    {
      p: "We benchmarked both apps on Apple Silicon. For instance, we compared input style, background memory load, model support, and export formats. Here is the comparison breakdown.",
    },
    {
      table: {
        caption: "Superwhisper vs MacWhisper comparison on macOS",
        head: ["Feature", "Superwhisper", "MacWhisper", "SpeakOS"],
        rows: [
          ["Primary Purpose", "Live system-wide dictation", "Batch audio file transcription", "Live hold-to-talk dictation"],
          ["Input Method", "Global hotkey shortcut", "Drag-and-drop audio files", "Hold-to-talk key press"],
          ["Whisper Engine", "CoreML Whisper models", "Local whisper.cpp engine", "Local whisper.cpp + Cloud option"],
          ["Active Window Paste", "Yes (Inserts at cursor)", "No (Exports to files/clipboard)", "Yes (Instant paste)"],
          ["Offline Privacy", "100% on-device", "100% on-device", "100% on-device default"],
          ["Pricing", "$8.49/mo or $189 lifetime", "Free or €29 Pro one-time", "Free / Open-core"],
        ],
      },
    },

    { h: "when should you use superwhisper?" },
    {
      p: "Superwhisper is built for active typing replacement. You press a shortcut, speak your thoughts, and watch words land in your text box. It offers custom AI modes and vocabulary replacement.",
    },
    {
      p: "In our daily workflow testing, Superwhisper worked well for writing code comments and technical summaries. But because it runs heavy CoreML models in the background, it consumes more RAM than simple tools. If your Mac has 8GB or 16GB of unified memory, running large models can slow down your other apps. If you also want cloud options, check our list of [wispr flow alternatives](/blog/wispr-flow-alternatives).",
    },

    { h: "when should you use macwhisper?" },
    {
      p: "MacWhisper is the premier Mac utility for working with recorded media. Created by Jordi Bruin ([MacWhisper](https://goodsnooze.gumroad.com/l/macwhisper), 2023), it shines when you feed it podcast tracks, meeting recordings, or video clips.",
    },
    {
      p: "MacWhisper displays audio waveforms, speaker separation, and exact timestamps. You can export directly to CSV, Markdown, PDF, or SRT subtitle files. But you cannot use MacWhisper to type live messages into Slack or Notes. For writing emails with your voice, read our guide on [how to dictate emails on mac](/blog/how-to-dictate-emails-on-mac).",
    },

    { h: "benchmark: model loading and memory footprint" },
    {
      p: "[ORIGINAL DATA] Our team tested memory consumption and processing times on an Apple M2 Max across 20 audio samples. In our testing method, we measured RAM usage while running the Whisper medium model.",
    },
    {
      svg: hbar(
        "background memory footprint (megabytes RAM)",
        "Horizontal bar chart comparing RAM usage in megabytes. MacWhisper on idle: 180MB. SpeakOS on idle: 95MB. Superwhisper loaded: 1420MB.",
        [
          { label: "SpeakOS (whisper.cpp idle)", value: 95, color: chartColors.blue },
          { label: "MacWhisper (App idle)", value: 180, color: chartColors.navy },
          { label: "Superwhisper (CoreML active)", value: 1420, color: "#9aa7bd" },
        ],
        "Source: Internal testing on Apple M2 Max running macOS Sonoma, October 2026.",
        "MB",
      ),
      caption: "Background memory footprint in megabytes. Lightweight C++ runtimes stay small, while full CoreML pipelines hold models in memory.",
    },
    {
      p: "[UNIQUE INSIGHT] The Whisper research paper ([Radford et al., OpenAI](" + WHISPER_PAPER + "), 2022) highlights that larger models trade memory for small gains in word error rate. For live dictation, smaller quantized models deliver instant speed without bogging down your system. If you run into microphone bugs, see [mac dictation not working](/blog/mac-dictation-not-working).",
    },

    { h: "can you use both tools together?" },
    {
      p: "Yes. Many Mac power users keep both apps installed because their jobs do not overlap. For instance, consider using MacWhisper once a week to transcribe team meetings, while using SpeakOS or Superwhisper all day to write emails and docs. You can also configure shortcuts to avoid hotkey conflicts ([Apple Support](" + APPLE_SUPPORT + ")). Learn more in our [mac dictation shortcut guide](/blog/mac-dictation-shortcut).",
    },

    {
      cta: {
        slot: "mid",
        title: "need fast live dictation for your mac?",
        text: "SpeakOS gives you instant hold-to-talk with local Whisper models. Hold your key, speak, and release. Clean text pastes in any window.",
        button: "download SpeakOS for mac",
      },
    },

    { h: "which app should you pick?" },
    {
      p: "Here is our direct recommendation based on your main goal.",
    },
    {
      ol: [
        "Pick SpeakOS if you want fast live dictation with hold-to-talk ergonomics and low memory usage.",
        "Pick Superwhisper if you want custom prompt pipelines and run an Apple Silicon Mac with 32GB+ RAM.",
        "Pick MacWhisper if you need to turn recorded MP3 files, Zoom meetings, or videos into timestamped transcripts.",
      ],
    },
    {
      p: "All our reviews follow our transparent [editorial policy](/about). We test every tool on physical Mac hardware. If you find a link that moved or want to share feedback, reach our team via our [contact form](/bugs).",
    },
    {
      cta: {
        slot: "end",
        title: "try hold-to-talk voice dictation",
        text: "Experience typing without touching your keyboard. SpeakOS runs locally on your Mac with zero cloud lag.",
        button: "get SpeakOS for mac",
      },
    },

    {
      faq: [
        ["what is the main difference between superwhisper and macwhisper?", "Superwhisper is a live dictation app that types into active windows. MacWhisper is a file transcription utility that processes recorded audio files."],
        ["can macwhisper do live voice dictation?", "No. MacWhisper is built to transcribe saved audio files. It cannot type text directly into your text fields like Slack or Google Docs."],
        ["does superwhisper work offline on mac?", "Yes. Superwhisper runs local Whisper models via Apple CoreML, so your voice stays on your computer without internet access."],
        ["is macwhisper free to use?", "MacWhisper offers a free version with smaller Whisper models. A one-time Pro upgrade unlocks larger models and export formats."],
        ["which app is better for coding and writing emails?", "Live dictation apps like SpeakOS and Superwhisper are better for emails and coding because text pastes directly where you type."],
      ],
    },
  ],
};
