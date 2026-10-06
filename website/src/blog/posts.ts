// Pure data, no React: the page renders it, and vite.config.ts bakes the same text into static
// HTML at build time so crawlers that never run JavaScript still read the post.
export type Block = { h: string } | { p: string } | { ul: string[] };

export type Post = {
  slug: string;
  title: string;
  description: string;
  date: string; // YYYY-MM-DD
  minutes: number;
  // A draft is served but noindex and left out of the sitemap and the blog index.
  draft?: boolean;
  blocks: Block[];
};

export const POSTS: Post[] = [
  {
    slug: "your-first-draft-should-be-spoken",
    title: "your first draft should be spoken.",
    description: "Most people speak faster than they type, and a spoken draft sounds like you. How to get words down first with SpeakOS and tidy them later.",
    date: "2026-10-05",
    minutes: 3,
    blocks: [
      { p: "you know what you want to say. the slow part is getting it out of your head and into a box, one key at a time. so say it first." },
      { h: "hold a key, talk" },
      { p: "with SpeakOS you hold a key and speak. when you let go, clean text lands in whatever app has focus: an email, a doc, a chat, a comment in your code. there is no window to switch to and nothing to paste." },
      { h: "why a spoken draft is a better draft" },
      {
        ul: [
          "it sounds like you. people write stiff and speak plainly, and plain is usually what the reader wanted.",
          "it gets you past the blank page. talking has momentum, and a rough paragraph is easier to fix than an empty one.",
          "you edit instead of invent. the hard part is done before you touch the keyboard.",
        ],
      },
      { h: "what clean means" },
      { p: "speech has ums, restarts and no punctuation. SpeakOS tidies that into text you could send, so you are fixing a word here and there, not retyping the whole thing." },
      { h: "keep it on your mac" },
      { p: "dictation runs on your mac by default, and it works with no internet. a faster cloud option is there if you want it. SpeakOS itself has no analytics and no telemetry." },
      { h: "try it on your next email" },
      { p: "hold the key and say the whole message. then read it back and fix what is off. most of the time there is less to fix than you expect." },
    ],
  },
];

export const publicPosts = () => POSTS.filter((p) => !p.draft);
export const postPath = (p: Post) => `/blog/${p.slug}`;
export const formatDate = (iso: string) => new Date(`${iso}T00:00:00Z`).toLocaleDateString("en-US", { year: "numeric", month: "short", day: "numeric", timeZone: "UTC" });
