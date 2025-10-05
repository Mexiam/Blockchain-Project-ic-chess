// src/ic-chess-frontend/src/App.jsx
import { useEffect, useState } from "react";
import Board from "./Board";
import { getActor } from "./agent";

export default function App() {
  const [view, setView] = useState("start"); // "start" | "game"
  const [flash, setFlash] = useState("");

  // If URL already has ?game, go straight to game view
  useEffect(() => {
    const url = new URL(location.href);
    if (url.searchParams.get("game")) setView("game");
  }, []);

  async function createAs(color /* "white" | "black" */) {
    setFlash("");
    try {
      const { actor } = await getActor();
      const res = await actor.create_game(); // (id, whiteToken, blackToken)
      const id = res[0];
      const white = res[1];
      const black = res[2];

      // store so the board can show the opposite invite
      localStorage.setItem(
        `icchess_tokens_${id}`,
        JSON.stringify({ white, black })
      );

      // hard navigate with the chosen token (board will auto-join)
      const base = `${location.origin}${location.pathname}`;
      const url = new URL(base);
      url.searchParams.set("game", id.toString());
      url.searchParams.set("token", color === "white" ? white : black);
      window.location.assign(url.toString());
    } catch (e) {
      setFlash(`Create failed: ${String(e)}`);
    }
  }

  async function joinExisting(e) {
    e.preventDefault();
    setFlash("");
    const form = new FormData(e.currentTarget);
    const idStr = (form.get("gid") || "").trim();
    const token = (form.get("token") || "").trim();
    if (!idStr) {
      setFlash("Enter a game id");
      return;
    }
    let id;
    try {
      id = BigInt(idStr);
    } catch {
      setFlash("Invalid game id");
      return;
    }
    const base = `${location.origin}${location.pathname}`;
    const url = new URL(base);
    url.searchParams.set("game", id.toString());
    if (token) url.searchParams.set("token", token);
    window.location.assign(url.toString());
  }

  if (view !== "game") {
    return (
      <div style={{ maxWidth: 640, margin: "40px auto", lineHeight: 1.5 }}>
        <h1>IC Chess</h1>
        {flash && (
          <pre style={{ color: "crimson", whiteSpace: "pre-wrap" }}>{flash}</pre>
        )}

        <p>Start a new on-chain game and invite a friend:</p>
        <div style={{ display: "flex", gap: 12, marginTop: 12 }}>
          <button onClick={() => createAs("white")}>Create game as White</button>
          <button onClick={() => createAs("black")}>Create game as Black</button>
        </div>

        <hr style={{ margin: "24px 0" }} />

        <form onSubmit={joinExisting}>
          <h3>Join existing game</h3>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr auto", gap: 8 }}>
            <input name="gid" placeholder="Game ID (e.g., 123)" />
            <input name="token" placeholder="(Optional) Seat token" />
            <button type="submit">Open</button>
          </div>
          <div style={{ fontSize: 12, opacity: 0.8, marginTop: 6 }}>
            Tip: paste a seat token to claim White/Black. Leave token empty to spectate.
          </div>
        </form>
      </div>
    );
  }

  return <Board />;
}
