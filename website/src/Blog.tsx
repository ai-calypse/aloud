import { useNavigate, useParams } from "react-router";
import readingCat from "./assets/memes/reading-cat.webp";
import { GlossyButton, MacWindow, PillLabel } from "./components/ds";
import SmartLink from "./components/SmartLink";
import Sticker from "./components/Sticker";
import { useCopy } from "./copy";
import { AUTHOR, POSTS, formatDate, postHtml, postPath, publicPosts } from "./blog/posts";
import NotFound from "./NotFound";
import { useMeta } from "./seo";

const page = "relative flex min-h-svh flex-col items-center gap-10 bg-blue-700 px-5 pb-24 pt-24 text-neutral-0";
const dots = { backgroundImage: "radial-gradient(circle at center, #0b57d6 1.6px, transparent 2px)", backgroundSize: "58px 58px" };

function Back({ href, children }: { href: string; children: string }) {
  return (
    <div className="absolute left-5 top-5">
      <GlossyButton size="sm" tone="white" href={href}>
        {children}
      </GlossyButton>
    </div>
  );
}

export function BlogIndex() {
  const c = useCopy();
  useMeta(c.blog.title, { description: c.blog.lede });
  const posts = publicPosts();
  return (
    <main className={page} style={dots}>
      <Back href="/">{c.common.backHome}</Back>
      <div className="flex w-full max-w-[760px] flex-col items-start gap-5">
        <PillLabel>{c.blog.pill}</PillLabel>
        <h1 className="t-section">{c.blog.h1}</h1>
        <p className="t-lede max-w-[520px]">{c.blog.lede}</p>
      </div>
      <MacWindow title="posts" className="w-full max-w-[760px]">
        <ul className="text-ink">
          {posts.length === 0 && <li className="p-6 text-ink-muted">{c.blog.empty}</li>}
          {posts.map((p) => (
            <li key={p.slug} className="border-t border-line first:border-t-0">
              <SmartLink href={postPath(p)} className="flex flex-col gap-1 p-6">
                <span className="t-card">{p.title}</span>
                <span className="text-ink-muted">{p.description}</span>
                <span className="text-[13px] text-ink-muted">{formatDate(p.date)}</span>
              </SmartLink>
            </li>
          ))}
        </ul>
      </MacWindow>
    </main>
  );
}

export function BlogPost() {
  const { slug } = useParams();
  const post = POSTS.find((p) => p.slug === slug);
  const c = useCopy();
  const navigate = useNavigate();
  useMeta(post ? post.metaTitle : c.blog.title, { description: post?.description, noindex: post?.draft });
  if (!post) return <NotFound />;
  return (
    <main className={page} style={dots}>
      <Back href="/blog">{c.blog.all}</Back>
      <header className="relative flex w-full max-w-[760px] flex-col items-start gap-5">
        <Sticker rotate={5} className="absolute -top-2 right-0 w-[96px] md:-right-16 md:top-0 md:w-[150px]">
          <img src={readingCat} alt="A close-up of an orange cat with a question mark above its head" />
        </Sticker>
        <PillLabel>{c.blog.pill}</PillLabel>
        <h1 className="t-section max-w-[640px]">{post.title}</h1>
        <p className="text-[14px] text-blue-100">
          {c.blog.by} <SmartLink href={AUTHOR.url} className="underline underline-offset-2">{AUTHOR.name}</SmartLink> · {formatDate(post.date)}
          {post.updated && ` · ${c.blog.updated} ${formatDate(post.updated)}`} · {c.blog.read(post.minutes)}
          {post.checked && (
            <>
              <br />
              {c.blog.checked} {formatDate(post.checked)}. <SmartLink href="/bugs" className="underline underline-offset-2">{c.blog.correct}</SmartLink>
            </>
          )}
        </p>
      </header>
      <MacWindow title={`${post.slug}.md`} className="w-full max-w-[760px]">
        <article
          className="post flex flex-col gap-5 p-6 text-[17px] leading-[1.65] text-ink md:p-10"
          // Our own copy, rendered by the same function that writes the static HTML. In-site links
          // go through the router so they do not reload the page.
          onClick={(e) => {
            const a = (e.target as Element).closest("a");
            const href = a?.getAttribute("href");
            if (!a || !href?.startsWith("/") || e.metaKey || e.ctrlKey || e.shiftKey) return;
            e.preventDefault();
            navigate(href);
          }}
        >
          <img src={post.cover.src} alt={post.cover.alt} width={1280} height={720} fetchPriority="high" className="block w-full rounded-[12px]" />
          <div className="contents" dangerouslySetInnerHTML={{ __html: postHtml(post) }} />
        </article>
      </MacWindow>
    </main>
  );
}
