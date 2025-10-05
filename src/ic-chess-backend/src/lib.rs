use candid::{CandidType, Deserialize, Principal};
use ic_cdk::{
    api::{caller, time, management_canister::main::raw_rand}, // raw_rand path is deprecated but OK for now
};
use ic_cdk_macros::{init, query, update};
use sha2::{Digest, Sha256};
use std::{cell::RefCell, collections::BTreeMap, str::FromStr};
use base64::Engine; // for .encode()

use shakmaty::{
    Chess, Position, Move as ShMove,
    san::San,
    fen::Fen,
    Color, Role, Square,
    EnPassantMode,
};

// -------------------- Public types (Candid) --------------------

#[derive(CandidType, Deserialize, Clone)]
pub enum GameStatus {
    Ongoing,
    Checkmate { winner_white: bool },
    Stalemate,
    Draw { reason: String },
    Resigned { winner_white: bool },
}

#[derive(candid::CandidType, serde::Deserialize, serde::Serialize, Clone)]
pub enum PlayerRole {
    White,
    Black,
    Spectator,
}

#[derive(CandidType, Deserialize, Clone)]
pub struct GameView {
    pub id: u64,
    pub white: Option<Principal>,
    pub black: Option<Principal>,
    pub fen: String,
    pub moves_san: Vec<String>,
    pub status: GameStatus,
    pub created_ns: u64,
    pub updated_ns: u64,
    pub to_move_white: bool,
    pub white_principal: Option<Principal>,
    pub black_principal: Option<Principal>,
}

// -------------------- Internal state --------------------

#[derive(Clone)]
struct GameInternal {
    id: u64,
    pos: Chess,
    moves_san: Vec<String>,
    white: Option<Principal>,
    black: Option<Principal>,
    // store only hashes on-chain, never raw tokens
    white_token_hash: [u8; 32],
    black_token_hash: [u8; 32],
    status: GameStatus,
    created_ns: u64,
    updated_ns: u64,
}

#[derive(Default)]
struct State {
    next_id: u64,
    games: BTreeMap<u64, GameInternal>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State {
        next_id: 1,
        games: BTreeMap::new(),
    });
}

// -------------------- Helpers --------------------

fn hash_token(s: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(s.as_bytes());
    let out = h.finalize();
    let mut arr = [0u8; 32];
    arr.copy_from_slice(&out);
    arr
}

async fn random_token() -> String {
    let (bytes,) = raw_rand().await.expect("raw_rand failed");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(&bytes)
}


fn to_view(g: &GameInternal) -> GameView {
    GameView {
        id: g.id,
        white: g.white,
        black: g.black,
        // shakmaty 0.29 signature
        fen: Fen::from_position(&g.pos, EnPassantMode::Legal).to_string(),
        moves_san: g.moves_san.clone(),
        status: g.status.clone(),
        created_ns: g.created_ns,
        updated_ns: g.updated_ns,
        white_principal: g.white,
        black_principal: g.black,
        to_move_white: matches!(g.pos.turn(), Color::White),
    }
}

fn compute_status(pos: &Chess) -> GameStatus {
    // Any legal moves?
    let mut has_any = false;
    for _ in pos.legal_moves() { has_any = true; break; }
    if !has_any {
        // No legal moves ⇒ checkmate or stalemate
        let in_check = !pos.checkers().is_empty();
        if in_check {
            let winner_white = !matches!(pos.turn(), Color::White);
            return GameStatus::Checkmate { winner_white };
        } else {
            return GameStatus::Stalemate;
        }
    }
    GameStatus::Ongoing
}

// Parse a simple UCI string like "e2e4" or "e7e8q"
fn parse_uci_to_move(pos: &Chess, mv: &str) -> Option<ShMove> {
    if mv.len() < 4 { return None; }
    let from = Square::from_str(&mv[0..2]).ok()?;
    let to = Square::from_str(&mv[2..4]).ok()?;
    let promo_role = if mv.len() == 5 {
        match &mv[4..5] {
            "q" | "Q" => Some(Role::Queen),
            "r" | "R" => Some(Role::Rook),
            "b" | "B" => Some(Role::Bishop),
            "n" | "N" => Some(Role::Knight),
            _ => None,
        }
    } else { None };

    for m in pos.legal_moves() {
        if m.from() == Some(from) && m.to() == to {
            if let Some(pr) = promo_role {
                if m.promotion() == Some(pr) { return Some(m); }
            } else {
                if m.promotion().is_none() { return Some(m); }
                // permissive default promo if omitted but required
                if m.promotion() == Some(Role::Queen) { return Some(m); }
            }
        }
    }
    None
}

/// Try UCI first, then SAN
fn parse_move_with_autopromo(pos: &Chess, mv: &str) -> Result<ShMove, String> {
    if let Some(m) = parse_uci_to_move(pos, mv) {
        return Ok(m);
    }
    if let Ok(san) = mv.parse::<San>() {
        return san.to_move(pos).map_err(|_| "Illegal move".into());
    }
    Err("Move must be SAN (e.g. 'e4') or UCI ('e2e4'/'e7e8q')".into())
}

// -------------------- Lifecycle --------------------

#[init]
fn init() {}

// -------------------- Queries --------------------

#[query]
fn get_game(id: u64) -> Option<GameView> {
    STATE.with(|s| s.borrow().games.get(&id).map(to_view))
}

#[query]
fn list_recent(offset_desc: u64, limit: u32) -> Vec<GameView> {
    STATE.with(|s| {
        let games = &s.borrow().games;
        let mut ids: Vec<_> = games.keys().cloned().collect();
        ids.sort_unstable_by(|a,b| b.cmp(a)); // newest first
        ids.into_iter()
            .skip(offset_desc as usize)
            .take(limit as usize)
            .filter_map(|id| games.get(&id).map(to_view))
            .collect()
    })
}

#[ic_cdk::query]
fn my_role(game_id: u64) -> PlayerRole {
    STATE.with(|s| {
        let who = caller();
        if let Some(g) = s.borrow().games.get(&game_id) {
            if g.white == Some(who) {
                PlayerRole::White
            } else if g.black == Some(who) {
                PlayerRole::Black
            } else {
                PlayerRole::Spectator
            }
        } else {
            PlayerRole::Spectator
        }
    })
}

/// Debug helper to inspect seats and token hashes (for testing)
#[ic_cdk::query]
fn debug_game(game_id: u64) -> Option<(Option<Principal>, Option<Principal>, [u8;32], [u8;32])> {
    STATE.with(|s| {
        s.borrow().games.get(&game_id).map(|g| {
            (g.white, g.black, g.white_token_hash, g.black_token_hash)
        })
    })
}

// -------------------- Updates --------------------

/// Create a new game. Returns (game_id, white_token, black_token).
#[update]
async fn create_game() -> (u64, String, String) {
    let now = time();
    let white_token = random_token().await;
    let black_token = random_token().await;

    let mut g = GameInternal {
        id: 0,
        pos: Chess::default(),
        moves_san: vec![],
        white: None,
        black: None,
        white_token_hash: hash_token(&white_token),
        black_token_hash: hash_token(&black_token),
        status: GameStatus::Ongoing,
        created_ns: now,
        updated_ns: now,
    };

    STATE.with(|s| {
        let mut s = s.borrow_mut();
        let id = s.next_id;
        s.next_id += 1;
        g.id = id;
        s.games.insert(id, g);
        (id, white_token, black_token)
    })
}

/// Claim a seat using a one-time token (burned on success)
#[ic_cdk::update]
async fn join_by_token(game_id: u64, token: String) -> Result<GameView, String> {
    STATE.with(|s| {
        let mut st = s.borrow_mut();
        let g = st.games.get_mut(&game_id).ok_or("No such game")?;
        let who = caller();

        // Already seated?
        if g.white == Some(who) || g.black == Some(who) {
            return Err("You already occupy a seat in this game".into());
        }

        let th = hash_token(&token);

        if th == g.white_token_hash {
            if g.white.is_some() {
                return Err("White seat already taken".into());
            }
            g.white = Some(who);
            g.white_token_hash = [0u8; 32]; // burn
            g.updated_ns = time();
            return Ok(to_view(g));
        } else if th == g.black_token_hash {
            if g.black.is_some() {
                return Err("Black seat already taken".into());
            }
            g.black = Some(who);
            g.black_token_hash = [0u8; 32]; // burn
            g.updated_ns = time();
            return Ok(to_view(g));
        }

        Err("Invalid or already-used token".into())
    })
}

#[update]
fn make_move(game_id: u64, mv: String) -> Result<GameView, String> {
    STATE.with(|s| {
        let who = caller();
        let mut st = s.borrow_mut();
        let g = st.games.get_mut(&game_id).ok_or("No such game")?;
        if !matches!(g.status, GameStatus::Ongoing) {
            return Err("Game finished".into());
        }

        // Enforce turn by seat (if a seat has been claimed)
        match g.pos.turn() {
            Color::White => if g.white.is_some() && g.white != Some(who) { return Err("Not white".into()); }
            Color::Black => if g.black.is_some() && g.black != Some(who) { return Err("Not black".into()); }
        }

        let m = parse_move_with_autopromo(&g.pos, &mv)?;
        let san_str = San::from_move(&g.pos, m).to_string();

        let new_pos = g.pos.clone().play(m).map_err(|_| "Illegal move")?;
        g.pos = new_pos;
        g.moves_san.push(san_str);

        g.status = compute_status(&g.pos);
        g.updated_ns = time();
        Ok(to_view(g))
    })
}

#[update]
fn resign(game_id: u64) -> Result<GameView, String> {
    STATE.with(|s| {
        let who = caller();
        let mut st = s.borrow_mut();
        let g = st.games.get_mut(&game_id).ok_or("No such game")?;
        if !matches!(g.status, GameStatus::Ongoing) {
            return Err("Game finished".into());
        }
        let winner_white = if g.white == Some(who) {
            false
        } else if g.black == Some(who) {
            true
        } else {
            return Err("You are not seated".into());
        };
        g.status = GameStatus::Resigned { winner_white };
        g.updated_ns = time();
        Ok(to_view(g))
    })
}

#[query]
fn export_pgn(game_id: u64) -> Result<String, String> {
    STATE.with(|s| {
        let binding = s.borrow();
        let g = binding.games.get(&game_id).ok_or("No such game")?;
        let mut pgn = String::new();
        pgn.push_str(&format!("[Event \"IC Chess {}\"]\n", game_id));
        pgn.push_str("[White \"?\"]\n[Black \"?\"]\n\n");
        let mut ply = 0usize;
        let mut move_no = 1usize;
        for san in &g.moves_san {
            if ply % 2 == 0 {
                pgn.push_str(&format!("{}. {} ", move_no, san));
                move_no += 1;
            } else {
                pgn.push_str(&format!("{} ", san));
            }
            ply += 1;
        }
        Ok(pgn)
    })
}

ic_cdk::export_candid!();
