import { useEffect } from "react";
import { BrowserRouter, Route, Routes, useLocation } from "react-router";
import Agents from "./Agents";
import { pageView } from "./analytics";
import Apps from "./Apps";
import { BlogIndex, BlogPost } from "./Blog";
import Bugs from "./Bugs";
import { BroToggle } from "./bro";
import Closing from "./Closing";
import ConsentBanner from "./ConsentBanner";
import { useCopy } from "./copy";
import Faq from "./Faq";
import Features from "./Features";
import Hero from "./Hero";
import MemeDesk from "./MemeDesk";
import NotFound from "./NotFound";
import Pricing from "./Pricing";
import Privacy from "./Privacy";
import { useMeta } from "./seo";
import Thanks from "./Thanks";
import Waitlist from "./Waitlist";

// Every navigation: back to the top (unless it is a #jump) and one page_view for analytics.
// Child effects run first, so the new page has already set its title by now.
function OnNavigate() {
  const { pathname, hash } = useLocation();
  useEffect(() => {
    if (!hash) window.scrollTo(0, 0);
    pageView(pathname);
  }, [pathname, hash]);
  return null;
}

function Home() {
  useMeta(useCopy().home.title);
  return (
    <>
      <Hero />
      <main>
        <MemeDesk />
        <Features />
        <Agents />
        <Apps />
        <Pricing />
        <Faq />
        <Closing />
      </main>
    </>
  );
}

export default function App() {
  return (
    <BrowserRouter>
      <OnNavigate />
      <Routes>
        <Route path="/" element={<Home />} />
        <Route path="/privacy" element={<Privacy />} />
        <Route path="/waitlist" element={<Waitlist />} />
        <Route path="/thanks" element={<Thanks />} />
        <Route path="/bugs" element={<Bugs />} />
        <Route path="/blog" element={<BlogIndex />} />
        <Route path="/blog/:slug" element={<BlogPost />} />
        <Route path="*" element={<NotFound />} />
      </Routes>
      <BroToggle />
      <ConsentBanner />
    </BrowserRouter>
  );
}
