import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import HistoryApp from "./history/HistoryApp";

const params = new URLSearchParams(window.location.search);
const isHistory = params.has("history");

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    {isHistory ? <HistoryApp /> : <App />}
  </React.StrictMode>,
);
