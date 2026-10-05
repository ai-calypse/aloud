import {
  CloudDrizzleIcon,
  CloudFogIcon,
  CloudIcon,
  CloudLightningIcon,
  CloudRainIcon,
  CloudSnowIcon,
  CloudSunIcon,
  SunIcon,
  type LucideIcon,
} from "lucide-react";
import type { Sky } from "@/lib/bindings";

/** A sky's glyph, shared by Home's weather tile and Today › Weather. */
export const SKY: Record<Sky, LucideIcon> = {
  clear: SunIcon,
  partlyCloudy: CloudSunIcon,
  cloudy: CloudIcon,
  fog: CloudFogIcon,
  drizzle: CloudDrizzleIcon,
  rain: CloudRainIcon,
  snow: CloudSnowIcon,
  storm: CloudLightningIcon,
};
