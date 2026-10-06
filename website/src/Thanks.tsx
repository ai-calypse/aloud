import player from "./assets/thanks-player.webp";
import thumbsCat from "./assets/memes/thumbs-cat.webp";
import poster from "./assets/memes/ty-poster.webp";
import video from "./assets/memes/ty.mp4";
import { GlossyButton } from "./components/ds";
import MemeVideo from "./components/MemeVideo";
import Sticker from "./components/Sticker";
import { useCopy } from "./copy";
import { useMeta } from "./seo";

export default function Thanks() {
  const c = useCopy().thanks;
  const home = useCopy().common.backHome;

  return (
    <main
      className="flex min-h-svh flex-col items-center justify-center gap-6 bg-blue-700 px-5 py-12 text-neutral-0"
      style={{ backgroundImage: "radial-gradient(circle at center, #0b57d6 1.6px, transparent 2px)", backgroundSize: "58px 58px" }}
    >
      {/* The picture has a Windows Media Player whose screen is cut out to transparent,
          so the cat plays behind it and the start menu still overlaps the corner.
          The video box is that hole, in % of the 735x801 image. */}
      <div className="relative w-full max-w-[min(560px,92vw,calc((100svh-230px)*0.918))] overflow-hidden rounded-[12px] shadow-window">
        <MemeVideo
          src={video}
          poster={poster}
          alt="A grey and white cat reaching a paw toward the camera, playing inside a Windows Media Player"
          className="absolute left-[14.42%] top-[28.59%] h-[39.7%] w-[59.05%] object-cover"
        />
        <img src={player} alt="" aria-hidden="true" width={735} height={801} className="relative block w-full" />
      </div>
      <div className="-mt-20 self-end pr-[max(0px,calc(50vw-300px))] max-md:pr-6">
        <Sticker rotate={-7} className="w-[92px]">
          <img src={thumbsCat} alt="A cat giving a thumbs up, with tears in its eyes" />
        </Sticker>
      </div>

      <div className="flex flex-col items-center gap-3 text-center">
        <h1 className="t-card">{c.h1}</h1>
        <p>{c.body}</p>
      </div>
      <div className="flex flex-wrap justify-center gap-4">
        <GlossyButton size="sm" tone="white" href="/">
          {home}
        </GlossyButton>
      </div>
    </main>
  );
}
