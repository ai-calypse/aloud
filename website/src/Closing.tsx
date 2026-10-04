import timeCat from "./assets/memes/time-cat.webp";
import tvWall from "./assets/tv-wall.webp";
import { GlossyButton } from "./components/ds";
import Sticker from "./components/Sticker";
import SmartLink from "./components/SmartLink";
import { useCopy } from "./copy";

export default function Closing() {
  const c = useCopy();
  return (
    <>
      <section
        id="download"
        className="overflow-x-clip px-5 py-[96px] text-center md:py-[140px]"
      >
        <div className="relative mx-auto max-w-[760px]">
          <h2 className="t-section">{c.closing.title}</h2>
          <p className="t-lede mt-4 text-ink-muted">{c.closing.lede}</p>
          <div className="mt-10 flex flex-wrap justify-center gap-4 max-md:[&_.al-gbtn]:h-auto! max-md:[&_.al-gbtn]:min-h-14! max-md:[&_.al-gbtn]:max-w-full! max-md:[&_.al-gbtn]:whitespace-normal! max-md:[&_.al-gbtn]:py-3! max-md:[&_.al-gbtn]:text-center! max-md:[&_.al-gbtn]:text-[19px]!">
            <div className="relative">
              <GlossyButton href="/thanks" icon="apple" event="download_click">
                {c.common.downloadMac}
              </GlossyButton>
              {/* stuck to the corner of the button, peeking over it */}
              <Sticker
                rotate={8}
                className="pointer-events-none absolute -right-[22px] -top-[34px] z-10 w-[54px]"
              >
                <img
                  src={timeCat}
                  alt="A cream cat pointing at a wristwatch, as if to say time is running out"
                  loading="lazy"
                />
              </Sticker>
            </div>
            <GlossyButton href="/waitlist" tone="white" icon="windows" event="waitlist_click">
              {c.common.windowsWaitlist}
            </GlossyButton>
          </div>
          <p className="mt-4 text-[13px] text-ink-muted">
            {c.common.macNote}
          </p>
        </div>
      </section>

      {/* The wall of tvs already spells "aloud", so it replaces the pixel wordmark.
          It fades into the page at the top; the links sit on its dark floor. */}
      <footer className="relative isolate overflow-hidden text-neutral-0">
        <img
          src={tvWall}
          alt="A wall of blue retro televisions whose screens spell out aloud, with a Windows Media Player style tv in front"
          width={2400}
          height={1339}
          loading="lazy"
          className="block w-full object-cover [mask-image:linear-gradient(to_bottom,transparent,black_16%)] md:aspect-[16/8] md:object-[50%_40%]"
        />
        <div className="bg-blue-950 px-5 pb-20 pt-5 md:absolute md:inset-x-0 md:bottom-0 md:bg-transparent md:bg-gradient-to-t md:from-blue-950 md:via-blue-950/70 md:to-transparent md:pb-6 md:pt-24">
          <p className="flex flex-wrap justify-center gap-x-3 text-[13px]">
            <span>aloud</span>·<span>{c.closing.madeFor}</span>·
            <SmartLink href="/privacy" className="link-u pb-0.5">
              {c.common.privacy}
            </SmartLink>
            ·
            <SmartLink href="/bugs" className="link-u pb-0.5">
              {c.closing.reportBug}
            </SmartLink>
          </p>
        </div>
      </footer>
    </>
  );
}
