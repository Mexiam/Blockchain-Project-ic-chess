// src/ic-chess-frontend/src/agent.js
import { HttpAgent, Actor } from "@dfinity/agent";
import { AuthClient } from "@dfinity/auth-client";
import {
  idlFactory as backendIdl,
  canisterId as backendCanisterId,
} from "../../declarations/ic-chess-backend";

const host = window.location.origin;
const isLocal = /localhost|127\.0\.0\.1/.test(host);

let cached = null;

/**
 * Minimal, always-works actor on local replica & ic.
 * - host = window.location.origin (frontend origin)
 * - fetchRootKey() on local to avoid "Invalid signature" errors
 */
export async function getActor() {
  if (cached) return cached;

  const auth = await AuthClient.create();
  // Keep whatever identity we currently have (anonymous or II)
  const identity = await auth.getIdentity();

  const agent = new HttpAgent({ host, identity });
  if (isLocal) {
    try {
      await agent.fetchRootKey();
    } catch (e) {
      console.warn("fetchRootKey failed; replica not reachable?", e);
    }
  }

  const actor = Actor.createActor(backendIdl, {
    agent,
    canisterId: backendCanisterId,
  });

  cached = { actor, agent, auth };
  return cached;
}

export async function principalText() {
  const { auth } = await getActor();
  try {
    return auth.getIdentity().getPrincipal().toText();
  } catch {
    return "anonymous";
  }
}
