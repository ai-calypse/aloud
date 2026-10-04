import type { AnchorHTMLAttributes } from "react";
import { Link } from "react-router";

// Site paths go through the router (no page reload); everything else, including #hash
// jumps and external URLs, stays a plain anchor.
export default function SmartLink({ href = "", ...rest }: AnchorHTMLAttributes<HTMLAnchorElement>) {
  return href.startsWith("/") ? <Link to={href} {...rest} /> : <a href={href} {...rest} />;
}
