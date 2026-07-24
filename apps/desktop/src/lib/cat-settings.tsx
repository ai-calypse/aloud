import { createContext, useContext } from "react";

/**
 * The Settings > Cats toggles, read by every place a cat could appear. A
 * context rather than prop-drilling: the cats show in ~six scattered spots
 * (the dictation notch, the ambient strip, the dealt cards, the agent list,
 * its detail, the hover peek), and "and other places we will include" means
 * more are coming — a consumer reads the flag with one hook, no new prop.
 *
 * Each getter already folds in the master switch, so a call site asks only the
 * question it cares about (`agents`, `dictation`) and never has to remember to
 * also check the master.
 */
export type CatSettings = {
  /** The master switch. */
  all: boolean;
  /** Master AND the dictation per-place switch. */
  dictation: boolean;
  /** Master AND the agents per-place switch. */
  agents: boolean;
};

// Default on: matches the Settings default, so a tree with no provider (a
// preview, a test) shows cats rather than hiding them.
const CatSettingsContext = createContext<CatSettings>({
  all: true,
  dictation: true,
  agents: true,
});

export const CatSettingsProvider = CatSettingsContext.Provider;

export function useCatSettings(): CatSettings {
  return useContext(CatSettingsContext);
}
