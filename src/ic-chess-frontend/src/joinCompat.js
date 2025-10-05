// joinCompat.js
export async function joinByTokenCompat(actor, id, token) {
  try {
    // Preferred/new shape: (nat64, text)
    return await actor.join_by_token(id, token);
  } catch (e) {
    const msg = String(e ?? "");
    // If the local candid/JS types still expect the old shape, retry with a record
    if (msg.includes("type mismatch")) {
      return await actor.join_by_token({ game_id: id, token });
    }
    throw e;
  }
}
