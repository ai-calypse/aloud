import React from "react";
import ReactDOM from "react-dom/client";
import { ListenBar } from "./features/listen/ListenBar";
import "./index.css";

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <ListenBar />
  </React.StrictMode>,
);
