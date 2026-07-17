import { useEffect, useState } from "react";
import { commands } from "./lib/bindings";

export default function App() {
  const [version, setVersion] = useState("");

  useEffect(() => {
    commands.appVersion().then(setVersion);
  }, []);

  return (
    <main>
      <h1>Aloud</h1>
      <p>v{version}</p>
    </main>
  );
}
