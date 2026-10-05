import { useRef, type ReactNode } from "react";
// The run's panel, the composer and the approval cards are styled here.
import "./styles/overlay.css";
import { VOICE_RUN_PREVIEWS } from "./features/overlay/previews";
import { CodingAgentComposer } from "./features/overlay/CodingAgentComposer";
import { VoiceBoards } from "./features/notch/views/widgets/Agents";

/**
 * Fixtures for Agents › Voice and Agents › Coding: past voice runs as a board,
 * with and without the run happening now, and the coding-agent composer. Labelled as fixtures; the app never shows them.
 * The compact run panel and the hover-opened Home are overlay states, not
 * components: the tuner's "Notch system" buttons play them in the real overlay.
 */

const noop = () => {};

function Block({ name, note, children }: { name: string; note: string; children: ReactNode }) {
  return (
    <section>
      <h2 className="m-0 text-notch-body font-semibold text-notch-ink">{name}</h2>
      <p className="m-0 mb-3 text-notch-meta text-notch-ink-3">{note}</p>
      {children}
    </section>
  );
}

export function CoherenceGallery() {
  const body = useRef<HTMLDivElement>(null);
  return (
    <div className="grid gap-9">
      <Block name="Agents › Voice" note="Recent in a Large tile, today and failed beside it">
        <div className="w-[780px] font-notch-w">
          <VoiceBoards run={null} runs={VOICE_RUN_PREVIEWS} bodyRef={body} onDecide={noop} onDelete={noop} />
        </div>
      </Block>
      <Block name="Agents › Voice · empty" note="one medium tile that teaches what fills it">
        <div className="w-[780px] font-notch-w">
          <VoiceBoards run={null} runs={[]} bodyRef={body} onDecide={noop} onDelete={noop} />
        </div>
      </Block>
      <Block name="Agents › Coding · New coding agent" note="the composer, in place of the list; × returns to it (the tuner's Shell · Coding button shows the list and its button)">
        <div className="w-[740px] font-notch text-notch-ink">
          <CodingAgentComposer bodyRef={body} onClose={noop} />
        </div>
      </Block>
    </div>
  );
}
