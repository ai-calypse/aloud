import errorCat from "./assets/memes/error-cat.webp";
import img from "./assets/404.webp";
import Sticker from "./components/Sticker";
import { useCopy } from "./copy";
import { useMeta } from "./seo";
import { GlossyButton, MacWindow } from "./components/ds";

// The 404 text is baked into the image; the page only adds a way home.
export default function NotFound() {
  const c = useCopy();
  useMeta("page not found · aloud", { noindex: true });
  return (
    <main className="flex min-h-svh flex-col items-center justify-center gap-10 px-5 py-12">
      <div className="relative w-full max-w-[760px]">
        <Sticker
          rotate={6}
          className="absolute -right-2 -top-14 z-10 w-[92px] md:-right-10 md:-top-16 md:w-[130px]"
        >
          <img
            src={errorCat}
            alt="A cat in sunglasses sipping a drink in a deckchair while the house behind it burns"
          />
        </Sticker>
        <MacWindow title="404.png" caption="404.png">
          <img
            src={img}
            alt="A sheep standing in a green valley below mountains, next to the text: 404, where am I"
            width={2400}
            height={1792}
            className="block w-full"
          />
        </MacWindow>
      </div>
      <GlossyButton href="/">{c.notFound.home}</GlossyButton>
    </main>
  );
}
