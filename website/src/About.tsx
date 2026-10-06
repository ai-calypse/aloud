import { useNavigate } from "react-router";
import { ABOUT, aboutHtml } from "./blog/about";
import { GlossyButton, MacWindow, PillLabel } from "./components/ds";
import { useCopy } from "./copy";
import { useMeta } from "./seo";

export default function About() {
  const c = useCopy();
  const navigate = useNavigate();
  useMeta(ABOUT.metaTitle, { description: ABOUT.description });
  return (
    <main
      className="relative flex min-h-svh flex-col items-center gap-10 bg-blue-700 px-5 pb-24 pt-24 text-neutral-0"
      style={{ backgroundImage: "radial-gradient(circle at center, #0b57d6 1.6px, transparent 2px)", backgroundSize: "58px 58px" }}
    >
      <div className="absolute left-5 top-5">
        <GlossyButton size="sm" tone="white" href="/">
          {c.common.backHome}
        </GlossyButton>
      </div>
      <div className="flex w-full max-w-[760px] flex-col items-start gap-5">
        <PillLabel>{c.blog.pill}</PillLabel>
        <h1 className="t-section max-w-[640px]">{ABOUT.title}</h1>
      </div>
      <MacWindow title="about.md" className="w-full max-w-[760px]">
        <article
          className="post flex flex-col gap-5 p-6 text-[17px] leading-[1.65] text-ink md:p-10"
          onClick={(e) => {
            const href = (e.target as Element).closest("a")?.getAttribute("href");
            if (!href?.startsWith("/") || e.metaKey || e.ctrlKey || e.shiftKey) return;
            e.preventDefault();
            navigate(href);
          }}
          dangerouslySetInnerHTML={{ __html: aboutHtml() }}
        />
      </MacWindow>
    </main>
  );
}
