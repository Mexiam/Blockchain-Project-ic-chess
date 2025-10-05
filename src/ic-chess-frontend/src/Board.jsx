import { useEffect, useRef, useState } from "react";
import { Chessground } from "chessground";
import "chessground/assets/chessground.base.css";
import "chessground/assets/chessground.brown.css";
import "chessground/assets/chessground.cburnett.css";
import { getActor, principalText } from "./agent";

// ----------- join_by_token helper (2-arg backend) -----------
async function joinByTokenCompat(actor, id, token) {
  try {
    // your Rust backend expects (nat64, text)
    return await actor.join_by_token(id, token);
  } catch (e) {
    throw new Error("join_by_token failed: " + e);
  }
}

// ----------- Small join form -----------
function JoinForm({ gameId, onAfterJoin }) {
  const [token, setToken] = useState("");
  const [msg, setMsg] = useState("");

  async function handleJoin() {
    try {
      const { actor } = await getActor();
      const r = await joinByTokenCompat(actor, gameId, token);
      if (r && r.Ok) {
        setMsg("Joined successfully!");
      } else if (r && r.Err) {
        setMsg("Join failed: " + r.Err);
      } else {
        setMsg("Join failed: unknown error");
      }
      if (onAfterJoin) await onAfterJoin(actor);
    } catch (err) {
      setMsg("Join failed: " + err);
    }
  }

  return (
    <div style={{ marginTop: 8 }}>
      <input
        value={token}
        onChange={(e) => setToken(e.target.value)}
        placeholder="Paste seat token"
        style={{ width: "60%" }}
      />
      <button onClick={handleJoin} style={{ marginLeft: 8 }}>Join</button>
      {msg && <div style={{ marginTop: 6 }}>{msg}</div>}
    </div>
  );
}

// ----------- Main Board component -----------
export default function Board({ gameId, shareTokens }) {
  const elRef = useRef(null);
  const cgRef = useRef(null);
  const actorRef = useRef(null);
  const gameRef = useRef(null);

  const [actor, setActor] = useState(null);
  const [game, setGame] = useState(null);
  const [role, setRole] = useState("Spectator");
  const [me, setMe] = useState("...");
  const [flash, setFlash] = useState("");

  // keep refs updated
  useEffect(() => { actorRef.current = actor; }, [actor]);
  useEffect(() => { gameRef.current = game; }, [game]);

  // -------- Initial load --------
  useEffect(() => {
    (async () => {
      const { actor: a } = await getActor();
      setActor(a);
      const meStr = await principalText();
      setMe(meStr);

      let id = gameId;
      if (!id) {
        const url = new URL(location.href);
        const g = url.searchParams.get("game");
        if (!g) { alert("No game ID in URL"); return; }
        id = BigInt(g);
      }

      const url = new URL(location.href);
      const token = url.searchParams.get("token");

      if (token) {
        try {
          await joinByTokenCompat(a, id, token);
        } catch (err) {
          setFlash("Auto-join failed: " + err);
        }
        url.searchParams.delete("token");
        history.replaceState(null, "", url.toString());
      }

      const got = await a.get_game(id);
      const g = got && got[0];
      if (!g) { alert("No such game"); return; }
      setGame(g);

      const rr = await a.my_role(id);
      const r =
        rr === "White" ? "White" :
        rr === "Black" ? "Black" : "Spectator";
      setRole(r);
    })();
  }, [gameId]);

  // -------- Mount Chessground --------
  useEffect(() => {
    if (!game || !elRef.current) return;

    const color =
      role === "White" ? "white" :
      role === "Black" ? "black" : null;

    const handleMove = async (from, to) => {
      const a = actorRef.current;
      const g = gameRef.current;
      if (!a || !g) return;

      const res = await a.make_move(g.id, from + to);
      if (res && res.Ok) {
        const newG = res.Ok;
        setGame(newG);
        cgRef.current.set({ fen: newG.fen });
      } else {
        cgRef.current.set({ fen: g.fen });
        alert(res?.Err || "Illegal move");
      }
    };

    if (!cgRef.current) {
      cgRef.current = Chessground(elRef.current, {
        fen: game.fen,
        orientation: color || "white",
        movable: { color: color, free: true },
        events: { move: handleMove },
      });
    } else {
      cgRef.current.set({
        fen: game.fen,
        orientation: color || "white",
        movable: { color: color, free: true },
      });
    }
  }, [game, role]);

  if (!game) return <div style={{ padding: 16 }}>Loading…</div>;

  // -------- Invite Links --------
  const base = `${location.origin}${location.pathname}`;
  const spectatorURL = `${base}?game=${game.id.toString()}`;

  const whiteInvite = shareTokens?.white
    ? `${base}?game=${game.id.toString()}&token=${shareTokens.white}` : "";
  const blackInvite = shareTokens?.black
    ? `${base}?game=${game.id.toString()}&token=${shareTokens.black}` : "";

  return (
    <div style={{ display: "grid", gridTemplateColumns: "minmax(320px,500px) 1fr", gap: 24 }}>
      <div><div ref={elRef} style={{ width: 500, height: 500 }} /></div>

      <div>
        <h2>Game #{game.id.toString()}</h2>
        <p><strong>You:</strong> {me} &nbsp; <strong>Role:</strong> {role}</p>
        {flash && <p style={{ color: "crimson" }}>{flash}</p>}
        <p><strong>FEN:</strong> {game.fen}</p>
        <p><strong>Moves:</strong> {game.moves_san.join(" ")}</p>

        <details open>
          <summary>Invites & join</summary>

          <p>Spectator link: <a href={spectatorURL}>{spectatorURL}</a></p>

          {whiteInvite && (
            <p>Invite as <strong>White</strong>: <a href={whiteInvite}>{whiteInvite}</a></p>
          )}
          {blackInvite && (
            <p>Invite as <strong>Black</strong>: <a href={blackInvite}>{blackInvite}</a></p>
          )}

          <div style={{ fontSize: 12, opacity: 0.8, marginBottom: 6 }}>
            Tokens are one-time. After a player successfully joins, the token is removed from the URL.
          </div>

          <JoinForm gameId={game.id} onAfterJoin={async (a) => {
            const got = await a.get_game(game.id);
            if (got && got[0]) setGame(got[0]);
            const rr = await a.my_role(game.id);
            setRole(rr);
          }} />
        </details>
      </div>
    </div>
  );
}

