use crate::pools::*;
use crate::rng::Rng;
use crate::sim::{advance_shuffle, sim_enc, sim_enc_weak, track_shuffle, unstable_shuffle};

// Base-34 alphabet (no I or O — matches SeedHelper._characters)
const SEED_ALPHABET: &[u8] = b"0123456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const SEED_BASE: u64 = 34;

pub fn int_to_seed_string(mut n: u64) -> String {
    if n == 0 {
        return "0".to_string();
    }
    let mut chars = Vec::new();
    while n > 0 {
        chars.push(SEED_ALPHABET[(n % SEED_BASE) as usize] as char);
        n /= SEED_BASE;
    }
    chars.reverse();
    chars.into_iter().collect()
}

// Deterministic hash — must match C# GetDeterministicHashCode exactly.
// Inputs are always ASCII, so str[i] == bytes[i] in C# UTF-16.
pub fn get_deterministic_hash_code(s: &str) -> i32 {
    let bytes = s.as_bytes();
    let mut h1: i32 = 352654597;
    let mut h2: i32 = h1;
    let mut i = 0usize;
    while i < bytes.len() {
        h1 = h1.wrapping_shl(5).wrapping_add(h1) ^ bytes[i] as i32;
        if i + 1 < bytes.len() {
            h2 = h2.wrapping_shl(5).wrapping_add(h2) ^ bytes[i + 1] as i32;
        }
        i += 2;
    }
    h1.wrapping_add(h2.wrapping_mul(1566083941))
}

pub fn make_run_seed(str_seed: &str) -> u32 {
    get_deterministic_hash_code(str_seed) as u32
}

pub fn make_rng_seed(run_seed: u32, name: &str) -> u32 {
    run_seed.wrapping_add(get_deterministic_hash_code(name) as u32)
}

// Returns true if this seed uses Underdocks as Act1.
// underdocks_revealed=true: UnderdocksEpoch unlocked (50/50 per seed).
// always_underdocks=true:   first time discovering Underdocks (always Underdocks).
pub fn is_underdocks_act1(str_seed: &str, underdocks_revealed: bool, always_underdocks: bool) -> bool {
    is_underdocks_from_hash(get_deterministic_hash_code(str_seed) as u32, underdocks_revealed, always_underdocks)
}

pub fn is_underdocks_from_hash(run_seed: u32, underdocks_revealed: bool, always_underdocks: bool) -> bool {
    if !underdocks_revealed {
        return false;
    }
    if always_underdocks {
        return true;
    }
    let mut act_rng = Rng::new(run_seed, 0);
    act_rng.next_bool()
}

// ---------------------------------------------------------------------------
// Ancient reward checks (separate per-event RNG streams)
// ---------------------------------------------------------------------------

// Orobas: 5 RNG calls, returns true if slot0 = ElectricShrymp (idx 0).
pub fn orobas_has_electric_shrymp(rng: &mut Rng, num_other_chars: i32, pool3_count: i32) -> bool {
    rng.next_item(num_other_chars); // character for SeaGlass
    rng.next_float();               // PrismaticGem check
    let slot0_pick = rng.next_item(4); // pool1: idx 0 = ElectricShrymp
    rng.next_item(3);               // pool2
    rng.next_item(pool3_count);     // pool3
    slot0_pick == 0
}

// Tanx: shuffles pool of `pool_size` weapons, ThrowingAxe=idx7 must land in top 3.
pub fn tanx_has_throwing_axe(rng: &mut Rng, pool_size: i32) -> bool {
    let mut indices: Vec<i32> = (0..pool_size).collect();
    unstable_shuffle(rng, &mut indices);
    indices[0] == 7 || indices[1] == 7 || indices[2] == 7
}

// Combined check: does Neow give EndOfDays via any path?
//
// Paths:
//   ArcaneScroll (positive opt 0): rewards_rng.NextItem(rare_card_count) == eod_rare_idx
//   LeafyPoultice (curse idx 2):   transformations_rng picks EndOfDays for Strike OR Defend
//   NewLeaf (positive opt 5/4):    niche_rng picks EndOfDays for whichever card player transforms
//
// Positive option indices (before removal): 0=ArcaneScroll, 1=BoomingConch, 2=Pomander,
//   3=GoldenPearl, 4=LeadPaperweight, 5=NewLeaf, 6=NeowsTorment, 7=PreciseScissors, 8=LostCoffer
// Curse list (solo+bundle): 0=CursedPearl, 1=LargeCapsule, 2=LeafyPoultice,
//   3=PrecariousShears, 4=Bundle, 5=Empower
// Removals: CursedPearl→removes GoldenPearl(3), LeafyPoultice→removes NewLeaf(5),
//           PrecariousShears→removes PreciseScissors(7)
//
// Transform pool for Basic cards (Strike/Defend/Bodyguard/Unleash):
//   GetFilteredTransformationOptions filters to C/U/R only (rarity 2-4).
//   C/U/R pool: 88 total - 4 Basic - 2 Ancient (ForbiddenGrimoire idx33, Protector idx54)
//              - 2 MultiplayerOnly (GlimpseBeyond idx35, LegionOfBone idx42) = 80.
//   EndOfDays (array index 27) → C/U/R position 25 (2 Basic cards before it are excluded).
//   Both Ancient cards and both MultiplayerOnly cards are after EndOfDays in the array, so positions are unaffected.
//   The basic card itself is NOT in the C/U/R pool, so the Id filter removes nothing.
//   This position is the same regardless of which basic card is being transformed.
//   LeafyPoultice: both Strike and Defend transforms use the same 80-card pool → EndOfDays at idx 25.
//   NewLeaf: player picks any deck card; any basic card transformed gives EndOfDays if niche == 25.
// Niche RNG: seeded from run_seed + hash("niche") [RunStateRng, no net_id].
// Transformations RNG: seeded from player_seed + hash("transformations") [PlayerRng].
/// Returns `Some(path_name)` if Neow yields EndOfDays via that path, `None` otherwise.
/// Path names: "ArcaneScroll", "LeafyPoultice", "NewLeaf".
pub fn neow_gives_end_of_days(
    neow_rng: &mut Rng,
    rewards_rng: &mut Rng,
    transformations_rng: &mut Rng,
    niche_rng: &mut Rng,
    curse_list_size: i32,
    rare_card_count: i32,
    eod_rare_idx: i32,
    transform_pool_size: i32,
    eod_transform_idx: i32,
) -> Option<&'static str> {
    // Basic-card transform pool: 80 C/U/R cards (Basic, Ancient, and MultiplayerOnly excluded).
    // EndOfDays is at eod_transform_idx (= 25) in the 80-card pool for all basic transforms.
    let basic_pool        = transform_pool_size;
    let eod_in_basic_pool = eod_transform_idx;

    // Fast pre-filter: check all downstream RNGs before the expensive Neow simulation.
    let rewards_eod      = rewards_rng.next_item(rare_card_count) == eod_rare_idx;
    let transform_s_pick = transformations_rng.next_item(basic_pool);
    let transform_s_eod  = transform_s_pick == eod_in_basic_pool;
    let transform_d_pick = transformations_rng.next_item(basic_pool);
    let transform_d_eod  = transform_d_pick == eod_in_basic_pool;
    let niche_pick_val   = niche_rng.next_item(basic_pool);
    let niche_eod        = niche_pick_val == eod_in_basic_pool;

    if !rewards_eod && !transform_s_eod && !transform_d_eod && !niche_eod {
        return None; // No path can yield EndOfDays
    }

    // Simulate Neow to confirm which option is actually offered.
    let curse_idx      = neow_rng.next_item(curse_list_size);
    let is_large_cap   = curse_idx == 1;
    let is_leafy_poul  = curse_idx == 2;
    // Removals: CursedPearl(0)→GoldenPearl(3), LeafyPoultice(2)→NewLeaf(5), PrecariousShears(3)→PreciseScissors(7)
    let has_removal    = curse_idx == 0 || curse_idx == 2 || curse_idx == 3;

    // NewLeaf's effective index after removal (None if removed by LeafyPoultice curse)
    let new_leaf_idx: Option<i32> = if is_leafy_poul {
        None            // NewLeaf removed
    } else if curse_idx == 0 {
        Some(4)         // GoldenPearl(3) removed → NewLeaf shifts 5→4
    } else {
        Some(5)         // NewLeaf stays at 5
    };

    let mut list_size = 9 - i32::from(has_removal);
    neow_rng.next_bool(); // Toughness or Safety
    list_size += 1;
    if !is_large_cap {
        neow_rng.next_bool(); // Patience or Scavenger
        list_size += 1;
    }
    let mut positions: Vec<i32> = (0..list_size).collect();
    unstable_shuffle(neow_rng, &mut positions);

    // LeafyPoultice path: automatic transforms of Strike (1st call) and Defend (2nd call).
    if is_leafy_poul {
        return if transform_s_eod || transform_d_eod { Some("LeafyPoultice") } else { None };
    }

    // ArcaneScroll path: ArcaneScroll is always at index 0.
    let arcane_offered = positions[0] == 0 || positions[1] == 0;
    if arcane_offered && rewards_eod {
        return Some("ArcaneScroll");
    }

    // NewLeaf path: any basic card in the deck can be transformed to get EndOfDays.
    // The pool is always 82 C/U/R cards regardless of which basic is chosen.
    if let Some(nl_idx) = new_leaf_idx {
        let new_leaf_offered = positions[0] == nl_idx || positions[1] == nl_idx;
        if new_leaf_offered && niche_eod {
            return Some("NewLeaf");
        }
    }

    None
}

// ---------------------------------------------------------------------------
// Defect Voltaic Neow check.
//
// Reuses neow_gives_end_of_days with Defect-specific card pool parameters:
//   ArcaneScroll pool: 25 Defect Rare cards (excl. MultiplayerOnly Ignition),
//                      Voltaic at index 24 (last alphabetically).
//   Transform pool:    80 C/U/R cards (88 total - 4 Basic - 2 Ancient - 2 MultiplayerOnly),
//                      Voltaic at index 78.
//
// UpFront start counter: 230 — same as Necrobinder because both character relic
// pools happen to have identical rarity counts (3 Rare, 2 Uncommon, 1 Common).
//
// Default constants (fully unlocked solo Defect):
pub const DEFECT_VOLTAIC_RARE_COUNT:    i32 = 25;
pub const DEFECT_VOLTAIC_RARE_IDX:      i32 = 24;
pub const DEFECT_VOLTAIC_TRANSFORM_IDX: i32 = 78;
// ---------------------------------------------------------------------------

/// Returns Some(path) if Neow gives Voltaic to a Defect player, None otherwise.
pub fn simulate_seed_defect_voltaic(
    run_seed: u32,
    net_id: u64,
    curse_list_size: i32,
    rare_card_count: i32,
    voltaic_rare_idx: i32,
    transform_pool_size: i32,
    voltaic_transform_idx: i32,
) -> Option<&'static str> {
    let player_seed          = run_seed.wrapping_add(net_id as u32);
    let rewards_seed         = make_rng_seed(player_seed, "rewards");
    let transformations_seed = make_rng_seed(player_seed, "transformations");
    let niche_seed           = make_rng_seed(run_seed, "niche");
    let neow_seed = (run_seed as u64)
        .wrapping_add(net_id)
        .wrapping_add(get_deterministic_hash_code("NEOW") as i64 as u64) as u32;

    let mut rewards_rng         = Rng::new(rewards_seed, 0);
    let mut transformations_rng = Rng::new(transformations_seed, 0);
    let mut niche_rng           = Rng::new(niche_seed, 0);
    let mut neow_rng            = Rng::new(neow_seed, 0);

    neow_gives_end_of_days(
        &mut neow_rng, &mut rewards_rng, &mut transformations_rng, &mut niche_rng,
        curse_list_size, rare_card_count, voltaic_rare_idx,
        transform_pool_size, voltaic_transform_idx,
    )
}

// ---------------------------------------------------------------------------
// Act3 boss pick — runs the UpFront RNG without any Necrobinder-specific
// condition checks (Orobas, Tanx, DollRoom, Reflections, Beacon).
// Used by the Defect Voltaic search to check the Act3 boss independently.
// ---------------------------------------------------------------------------

pub fn get_act3_boss(
    run_seed: u32,
    darv_epoch: bool,
    orobas_epoch: bool,
    neow_epoch: bool,
    underdocks_revealed: bool,
    always_underdocks: bool,
) -> i32 {
    let underdocks = is_underdocks_from_hash(run_seed, underdocks_revealed, always_underdocks);
    let up_front_seed = make_rng_seed(run_seed, "up_front");
    let mut rng = Rng::new(up_front_seed, 230);

    let shared_count = i32::from(darv_epoch);
    let act2_shared = rng.next_int(shared_count + 1);
    let _act3_shared = rng.next_int(shared_count - act2_shared + 1);

    // Act1 — consume event queue shuffle + encounters (same RNG cost as doll sim)
    if underdocks {
        advance_shuffle(&mut rng, UDK.events);
        sim_enc(&mut rng, &UDK);
    } else {
        advance_shuffle(&mut rng, OVG.events);
        sim_enc(&mut rng, &OVG);
    }
    rng.next_item(i32::from(neow_epoch));

    // Act2 — event queue + encounters + ancient pick
    advance_shuffle(&mut rng, HIVE.events);
    sim_enc(&mut rng, &HIVE);
    let act2_ancient_base = if orobas_epoch { 3 } else { 2 };
    rng.next_item(act2_ancient_base + act2_shared);

    // Act3 — event queue + encounters → boss returned by sim_enc
    advance_shuffle(&mut rng, GLORY.events);
    sim_enc(&mut rng, &GLORY)
}

// ---------------------------------------------------------------------------
// Dolly's Mirror: back-distance in the Shop deque (0 = appears in first shop).
//
// Uses UpFront RNG starting at counter=0. The shop deque shuffle is calls
// 205..229 (the last 25 of the 230 relic-bag initialisation calls).
// ---------------------------------------------------------------------------
pub fn dolly_mirror_back_distance(run_seed: u32) -> i32 {
    let up_front_seed = make_rng_seed(run_seed, "up_front");
    let mut rng = Rng::new(up_front_seed, 0);

    // SharedRelicGrabBag (112 calls total):
    advance_shuffle(&mut rng, 30); // Uncommon  → 29 calls
    advance_shuffle(&mut rng, 25); // Common    → 24 calls
    advance_shuffle(&mut rng, 35); // Rare      → 34 calls
    advance_shuffle(&mut rng, 25); // Shop      → 24 calls
    // Event(1) → 0 calls
    advance_shuffle(&mut rng, 2);  // Ancient   →  1 call  [counter = 112]

    // PlayerRelicGrabBag:
    advance_shuffle(&mut rng, 32); // Uncommon  → 31 calls  [counter = 143]
    advance_shuffle(&mut rng, 26); // Common    → 25 calls  [counter = 168]
    advance_shuffle(&mut rng, 38); // Rare      → 37 calls  [counter = 205]
    // Shop deque (26 relics, 25 calls) — track DollysMirror [counter = 230]
    let final_pos = track_shuffle(&mut rng, SHOP_DEQUE_SIZE, DOLLYS_MIRROR_IDX);
    (SHOP_DEQUE_SIZE - 1) - final_pos
}

// ---------------------------------------------------------------------------
// UnsettlingLamp grab bag positions and availability check.
//
// SharedRelicGrabBag Rare: 35 relics, UnsettlingLamp at array index 31.
// PlayerRelicGrabBag Rare: 38 relics (SharedRelicPool 35 + Necrobinder 3),
//   UnsettlingLamp stays at index 31.
//
// `unsettling_lamp_positions` returns (shared_rare_pos, player_rare_pos):
//   pos 0 = front of the deque (first draw of that rarity gets it).
//
// `unsettling_lamp_positions` returns (shared_rare_pos, player_rare_pos):
//   shared_rare_pos: position in SharedRelicGrabBag Rare deque → drawn by treasure rooms.
//   player_rare_pos: position in PlayerRelicGrabBag Rare deque → drawn by elites and shop.
//   pos 0 = front of the deque (first draw of that rarity gets it).
// ---------------------------------------------------------------------------
pub fn unsettling_lamp_positions(run_seed: u32) -> (i32, i32) {
    let up_front_seed = make_rng_seed(run_seed, "up_front");
    let mut rng = Rng::new(up_front_seed, 0);

    // SharedRelicGrabBag:
    advance_shuffle(&mut rng, 30); // Uncommon → 29 calls
    advance_shuffle(&mut rng, 25); // Common   → 24 calls
    // Rare (35 relics, 34 shuffle calls) — track UnsettlingLamp (index 31)
    let shared_rare_pos = track_shuffle(&mut rng, SHARED_RARE_SIZE, UNSETTLING_LAMP_SHARED_IDX);
    advance_shuffle(&mut rng, 25); // Shop     → 24 calls
    // Event (1 element) → 0 calls
    advance_shuffle(&mut rng, 2);  // Ancient  →  1 call  [counter = 112]

    // PlayerRelicGrabBag:
    advance_shuffle(&mut rng, 32); // Uncommon → 31 calls  [counter = 143]
    advance_shuffle(&mut rng, 26); // Common   → 25 calls  [counter = 168]
    // Rare (38 relics, 37 shuffle calls) — track UnsettlingLamp (index 31)
    let player_rare_pos = track_shuffle(&mut rng, PLAYER_RARE_SIZE, UNSETTLING_LAMP_PLAYER_IDX);

    (shared_rare_pos, player_rare_pos)
}

/// Returns which treasure room visit (1-indexed) yields UnsettlingLamp,
/// by simulating TreasureRoomRelics rarity rolls until the (shared_pos+1)th rare.
pub fn unsettling_lamp_treasure_visit(run_seed: u32, shared_pos: i32) -> i32 {
    let tr_seed = make_rng_seed(run_seed, "treasure_room_relics");
    let mut tr_rng = Rng::new(tr_seed, 0);
    let mut n_rare = 0i32;
    let mut visit = 0i32;
    loop {
        visit += 1;
        if tr_rng.next_float() >= 0.83 {
            n_rare += 1;
            if n_rare > shared_pos {
                return visit;
            }
        }
    }
}

// Returns (from_treasure, from_elite, from_shop, shared_rare_pos, player_rare_pos).
//   from_treasure: UnsettlingLamp drawn within `treasure_tries` treasure room visits.
//   from_elite:    elite_pos ≤ lamp_elite_pos_max, where elite_pos = player_rare_pos (from front).
//   from_shop:     shop_pos ≤ lamp_shop_pos_max, where shop_pos = (PLAYER_RARE_SIZE-1) - player_rare_pos (from back).
//   Disabled when the respective max is < 0.
pub fn unsettling_lamp_available(
    run_seed: u32,
    treasure_tries: i32,
    lamp_elite_pos_max: i32,
    lamp_shop_pos_max: i32,
) -> (bool, bool, bool, i32, i32) {
    let (shared_rare_pos, player_rare_pos) = unsettling_lamp_positions(run_seed);

    // Treasure Room check: TreasureRoomRelics RNG decides rarity for each treasure room visit.
    // Count how many of the first `treasure_tries` visits roll Rare (≥0.83).
    // UnsettlingLamp is drawn if that count exceeds its position in the Rare deque.
    let from_treasure = if treasure_tries > 0 {
        let tr_seed = make_rng_seed(run_seed, "treasure_room_relics");
        let mut tr_rng = Rng::new(tr_seed, 0);
        let mut n_rare = 0i32;
        for _ in 0..treasure_tries {
            if tr_rng.next_float() >= 0.83 {
                n_rare += 1;
            }
        }
        n_rare > shared_rare_pos
    } else {
        false
    };

    // Elite check: pulls from front of PlayerRelicGrabBag Rare deque.
    let from_elite = lamp_elite_pos_max >= 0 && player_rare_pos <= lamp_elite_pos_max;

    // Shop check: pulls from back of PlayerRelicGrabBag Rare deque.
    let shop_pos = (PLAYER_RARE_SIZE - 1) - player_rare_pos;
    let from_shop = lamp_shop_pos_max >= 0 && shop_pos <= lamp_shop_pos_max;

    (from_treasure, from_elite, from_shop, shared_rare_pos, player_rare_pos)
}

// ---------------------------------------------------------------------------
// Run the full UpFront RNG simulation for one seed. Returns the boss idx for
// each act and the positions of tracked events. All filtering is in search.rs.
// ---------------------------------------------------------------------------
pub struct SeedResult {
    pub underdocks: bool,
    pub dm_dist: i32,
    pub drowning_beacon_pos: i32, // only valid if underdocks
    pub doll_room_pos: i32,
    pub hive_weak_picks: [i32; 2],
    pub act3_boss_idx: i32,
    pub reflections_pos: i32,
    pub act2_is_orobas: bool,
    pub act3_is_tanx: bool,
}

// Conditions tracked (in order). Stage N means conditions 0..N-1 all passed.
// STAGE_FULL = 12 means all conditions passed.
pub const NUM_CONDITIONS: usize = 12;
pub const STAGE_FULL: u8 = NUM_CONDITIONS as u8;

pub const CONDITION_NAMES: [&str; NUM_CONDITIONS] = [
    // Cheap independent checks (checked in this order before UpFront simulation)
    "Tanx: ThrowingAxe",              // 0
    "Orobas: ElectricShrymp",         // 1
    "Neow gives EndOfDays",           // 2
    // UpFront simulation (sequential RNG stream)
    "DrowningBeacon pos OK",          // 3  (N/A when constraint disabled)
    "DollRoom pos OK",                // 4
    "ThievingHopper 2nd weak",        // 5  (N/A when disabled)
    "Act2 ancient = Orobas",          // 6
    "Reflections pos OK",             // 7
    "Act3 boss matches",               // 8  (N/A when act3_boss_idx < 0)
    "Act3 ancient = Tanx",            // 9
    // Deferred expensive checks
    "DollysMirror ≤ max shops",       // 10
    "UnsettlingLamp available",       // 11 (N/A when both lamp params disabled)
];

/// Hash-mode entry point: takes run_seed (u32) directly, skipping string hashing.
pub fn simulate_seed_doll_by_hash(
    run_seed: u32,
    net_id: u64,
    darv_epoch: bool,
    orobas_epoch: bool,
    neow_epoch: bool,
    num_other_chars: i32,
    orobas_pool3_count: i32,
    tanx_pool_size: i32,
    neow_curse_list_size: i32,
    rare_card_count: i32,
    end_of_days_rare_idx: i32,
    transform_pool_size: i32,
    end_of_days_transform_idx: i32,
    underdocks_revealed: bool,
    always_underdocks: bool,
    max_act1_shops: i32,
    drowning_beacon_max_pos: i32,
    hopper_second: bool,
    doll_room_max_pos: i32,
    reflections_max_pos: i32,
    lamp_treasure_tries: i32,
    lamp_elite_pos_max: i32,
    lamp_shop_pos_max: i32,
    no_lamp: bool,
    act3_boss_idx: i32,
) -> (u8, bool, i32, i32, i32, i32, &'static str) {
    simulate_seed_doll_inner(
        run_seed, net_id, darv_epoch, orobas_epoch, neow_epoch,
        num_other_chars, orobas_pool3_count, tanx_pool_size,
        neow_curse_list_size, rare_card_count, end_of_days_rare_idx,
        transform_pool_size, end_of_days_transform_idx,
        underdocks_revealed, always_underdocks, max_act1_shops,
        drowning_beacon_max_pos, hopper_second, doll_room_max_pos,
        reflections_max_pos, lamp_treasure_tries, lamp_elite_pos_max, lamp_shop_pos_max,
        no_lamp, act3_boss_idx,
    )
}

/// Returns (stage, underdocks, dm_dist, drowning_beacon_pos, doll_pos, refl_pos, neow_path).
/// stage = number of conditions passed (0–11). STAGE_FULL (12) = full match.
/// All fields beyond stage are only meaningful when stage == STAGE_FULL.
/// drowning_beacon_pos is -1 for Overgrowth seeds (DrowningBeacon doesn't exist there).
pub fn simulate_seed_doll(
    str_seed: &str,
    net_id: u64,
    darv_epoch: bool,
    orobas_epoch: bool,
    neow_epoch: bool,
    num_other_chars: i32,
    orobas_pool3_count: i32,
    tanx_pool_size: i32,
    neow_curse_list_size: i32,
    rare_card_count: i32,
    end_of_days_rare_idx: i32,
    transform_pool_size: i32,
    end_of_days_transform_idx: i32,
    underdocks_revealed: bool,
    always_underdocks: bool,
    max_act1_shops: i32,
    drowning_beacon_max_pos: i32,
    hopper_second: bool,
    doll_room_max_pos: i32,
    reflections_max_pos: i32,
    lamp_treasure_tries: i32,
    lamp_elite_pos_max: i32,
    lamp_shop_pos_max: i32,
    no_lamp: bool,
    act3_boss_idx: i32,
) -> (u8, bool, i32, i32, i32, i32, &'static str) {
    simulate_seed_doll_inner(
        make_run_seed(str_seed), net_id, darv_epoch, orobas_epoch, neow_epoch,
        num_other_chars, orobas_pool3_count, tanx_pool_size,
        neow_curse_list_size, rare_card_count, end_of_days_rare_idx,
        transform_pool_size, end_of_days_transform_idx,
        underdocks_revealed, always_underdocks, max_act1_shops,
        drowning_beacon_max_pos, hopper_second, doll_room_max_pos,
        reflections_max_pos, lamp_treasure_tries, lamp_elite_pos_max, lamp_shop_pos_max,
        no_lamp, act3_boss_idx,
    )
}

#[allow(clippy::too_many_arguments)]
fn simulate_seed_doll_inner(
    run_seed: u32,
    net_id: u64,
    darv_epoch: bool,
    orobas_epoch: bool,
    neow_epoch: bool,
    num_other_chars: i32,
    orobas_pool3_count: i32,
    tanx_pool_size: i32,
    neow_curse_list_size: i32,
    rare_card_count: i32,
    end_of_days_rare_idx: i32,
    transform_pool_size: i32,
    end_of_days_transform_idx: i32,
    underdocks_revealed: bool,
    always_underdocks: bool,
    max_act1_shops: i32,
    drowning_beacon_max_pos: i32,
    hopper_second: bool,
    doll_room_max_pos: i32,
    reflections_max_pos: i32,
    lamp_treasure_tries: i32,
    lamp_elite_pos_max: i32,
    lamp_shop_pos_max: i32,
    no_lamp: bool,
    act3_boss_idx: i32,
) -> (u8, bool, i32, i32, i32, i32, &'static str) {
    let fail = |stage| (stage, false, 0, -1, 0, 0, "");

    // --- Condition 0: Tanx offers ThrowingAxe ---
    // Cheap (~10 RNG calls), ~10% pass rate — best filter-per-cost, run first.
    // C#: (uint)(runSeed + netId + (ulong)hash) — (ulong)int sign-extends through i64
    let tanx_seed = (run_seed as u64)
        .wrapping_add(net_id)
        .wrapping_add(get_deterministic_hash_code("TANX") as i64 as u64) as u32;
    let mut tanx_rng = Rng::new(tanx_seed, 0);
    if !tanx_has_throwing_axe(&mut tanx_rng, tanx_pool_size) {
        return fail(0);
    }

    // --- Condition 1: Orobas offers ElectricShrymp ---
    // Cheap (~5 RNG calls), ~25% pass rate.
    let orobas_seed = (run_seed as u64)
        .wrapping_add(net_id)
        .wrapping_add(get_deterministic_hash_code("OROBAS") as i64 as u64) as u32;
    let mut orobas_rng = Rng::new(orobas_seed, 0);
    if !orobas_has_electric_shrymp(&mut orobas_rng, num_other_chars, orobas_pool3_count) {
        return fail(1);
    }

    // --- Condition 2: Neow gives EndOfDays (any path) ---
    // Expensive (4× Rng::new + simulation), ~1% pass rate.
    // Deferred until after cheap Tanx/Orobas filters eliminate ~93% of seeds.
    // PlayerRng streams: rewards and transformations use player_seed = run_seed + net_id [u32]
    // RunStateRng stream: niche uses run_seed directly (no net_id)
    // Event RNG (neow_seed): run_seed + net_id + hash("NEOW") [u64 + sign-extension]
    let neow_path = {
        let player_seed = run_seed.wrapping_add(net_id as u32);
        let rewards_seed         = make_rng_seed(player_seed, "rewards");
        let transformations_seed = make_rng_seed(player_seed, "transformations");
        let niche_seed           = make_rng_seed(run_seed, "niche");
        let neow_seed = (run_seed as u64)
            .wrapping_add(net_id)
            .wrapping_add(get_deterministic_hash_code("NEOW") as i64 as u64) as u32;
        let mut rewards_rng         = Rng::new(rewards_seed, 0);
        let mut transformations_rng = Rng::new(transformations_seed, 0);
        let mut niche_rng           = Rng::new(niche_seed, 0);
        let mut neow_rng            = Rng::new(neow_seed, 0);
        let Some(path) = neow_gives_end_of_days(
            &mut neow_rng, &mut rewards_rng, &mut transformations_rng, &mut niche_rng,
            neow_curse_list_size, rare_card_count, end_of_days_rare_idx,
            transform_pool_size, end_of_days_transform_idx,
        ) else {
            return fail(2);
        };
        path
    };

    // UpFront RNG — counter starts at 230 (relic-bag init consumed calls 0..229)
    let underdocks = is_underdocks_from_hash(run_seed, underdocks_revealed, always_underdocks);
    let up_front_seed = make_rng_seed(run_seed, "up_front");
    let mut rng = Rng::new(up_front_seed, 230);

    let shared_count = i32::from(darv_epoch);
    let act2_shared = rng.next_int(shared_count + 1);
    let act3_shared = rng.next_int(shared_count - act2_shared + 1);

    // Act1 GenerateRooms
    // --- Condition 3: DrowningBeacon pos OK (N/A when constraint disabled) ---
    let mut drowning_beacon_pos = -1i32;
    if underdocks {
        let db_pos = track_shuffle(&mut rng, UDK.events, DROWNING_BEACON_UDK_IDX);
        drowning_beacon_pos = db_pos;
        if drowning_beacon_max_pos > 0 && (db_pos == 0 || db_pos > drowning_beacon_max_pos) {
            sim_enc(&mut rng, &UDK);
            rng.next_item(i32::from(neow_epoch));
            return fail(3);
        }
        sim_enc(&mut rng, &UDK);
    } else {
        if drowning_beacon_max_pos > 0 {
            return fail(3); // beacon constraint requires Underdocks
        }
        track_shuffle(&mut rng, OVG.events, DOLL_ROOM_OVG_IDX);
        sim_enc(&mut rng, &OVG);
    }
    rng.next_item(i32::from(neow_epoch));

    // Act2 GenerateRooms
    // --- Condition 4: DollRoom pos in range (checked before running encounters) ---
    let doll_room_pos = track_shuffle(&mut rng, HIVE.events, DOLL_ROOM_HIVE_IDX);
    if doll_room_pos == 0 || doll_room_pos > doll_room_max_pos {
        return fail(4);
    }

    let (hive_weak_picks, _) = sim_enc_weak::<2>(&mut rng, &HIVE);
    let act2_ancient_base = if orobas_epoch { 3 } else { 2 };
    let act2_ancient_idx = rng.next_item(act2_ancient_base + act2_shared);
    let act2_is_orobas = orobas_epoch && act2_ancient_idx == 0;

    // --- Condition 5: ThievingHopper is 2nd weak (N/A when disabled) ---
    if hopper_second && hive_weak_picks[1] != THIEVES_HOPPER_WEAK_IDX {
        return fail(5);
    }

    // --- Condition 6: Act2 = Orobas ---
    if !act2_is_orobas {
        return fail(6);
    }

    // Act3 GenerateRooms
    // --- Condition 7: Reflections pos in range (checked before running encounters) ---
    let reflections_pos = track_shuffle(&mut rng, GLORY.events, REFLECTIONS_GLORY_IDX);
    if reflections_pos == 0 || reflections_pos > reflections_max_pos {
        return fail(7);
    }

    let act3_boss_result = sim_enc(&mut rng, &GLORY);

    // --- Condition 8: Act3 boss matches required index (-1 = any boss accepted) ---
    if act3_boss_idx >= 0 && act3_boss_result != act3_boss_idx {
        return fail(8);
    }

    let act3_ancient_idx = rng.next_item(3 + act3_shared);

    // --- Condition 9: Act3 = Tanx ---
    if act3_ancient_idx != 1 {
        return fail(9);
    }

    // --- Condition 10: Dolly's Mirror back-distance ≤ max shops ---
    // Deferred to last: costs ~230 RNG advances; almost no seeds reach here.
    let dm_dist = dolly_mirror_back_distance(run_seed);
    if dm_dist > max_act1_shops {
        return fail(10);
    }

    // --- Condition 11: UnsettlingLamp available (Treasure, Elite, or Shop) ---
    // Disabled when no_lamp=true or all three lamp params are at their defaults.
    if !no_lamp && (lamp_treasure_tries > 0 || lamp_elite_pos_max >= 0 || lamp_shop_pos_max >= 0) {
        let (from_treasure, from_elite, from_shop, _, _) =
            unsettling_lamp_available(run_seed, lamp_treasure_tries, lamp_elite_pos_max, lamp_shop_pos_max);
        if !from_treasure && !from_elite && !from_shop {
            return fail(11);
        }
    }

    (STAGE_FULL, underdocks, dm_dist, drowning_beacon_pos, doll_room_pos, reflections_pos, neow_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(seed: &str, doll_pos: i32, refl_pos: i32, shops: i32) -> (u8, bool, i32, i32, i32, i32, &'static str) {
        simulate_seed_doll(
            seed, 1, true, true, true, 4, 2, 10, 6, 25, 5, 82, 25,
            true, false, shops, 0, false, doll_pos, refl_pos, 0, -1, -1, false, QUEEN_BOSS_IDX,
        )
    }

    // C# confirmed: 3JJBXD passes --doll-pos 3 --reflect-pos 5 --max-shops 3
    // DollRoom pos=3, Reflections pos=2, dm_dist=2, underdocks=true
    #[test]
    fn seed_3jjbxd_passes() {
        let (stage, underdocks, dm_dist, _db_pos, doll_pos, refl_pos, _neow) = check("3JJBXD", 3, 5, 3);
        assert_eq!(stage, STAGE_FULL, "3JJBXD should be a full match");
        assert!(underdocks,       "3JJBXD should be Underdocks");
        assert_eq!(dm_dist,  2,   "3JJBXD dm_dist should be 2");
        assert_eq!(doll_pos, 3,   "3JJBXD doll_room_pos should be 3");
        assert_eq!(refl_pos, 2,   "3JJBXD reflections_pos should be 2");
    }

    // 3JJBXD: dm_dist=2, so max_shops=1 should fail cond 3 (DollysMirror).
    // Use doll_pos=3, refl_pos=2 so conditions 4-10 pass, hitting the deferred check.
    #[test]
    fn seed_3jjbxd_fails_tight() {
        let (stage, ..) = check("3JJBXD", 3, 2, 1);
        assert_eq!(stage, 10, "3JJBXD should fail cond 10 (dm_dist=2 > max_shops=1)");
    }

    // QDHB0L: Act2=Orobas, Act3=Vakuu → fails cond 9 (Act3=Tanx)
    #[test]
    fn seed_qdhb0l_fails_tanx() {
        let (stage, ..) = check("QDHB0L", 3, 5, 3);
        assert_eq!(stage, 9, "QDHB0L should fail at cond 9 (Act3=Vakuu, not Tanx)");
    }

    #[test]
    fn trace_qa4auq9_neow() {
        let str_seed = "QA4AUQ9";
        let net_id = 1u64;
        let run_seed = make_run_seed(str_seed);
        let player_seed = run_seed.wrapping_add(net_id as u32);
        let rewards_seed = make_rng_seed(player_seed, "rewards");
        let transformations_seed = make_rng_seed(player_seed, "transformations");
        let niche_seed = make_rng_seed(run_seed, "niche");
        let neow_seed = (run_seed as u64)
            .wrapping_add(net_id)
            .wrapping_add(get_deterministic_hash_code("NEOW") as i64 as u64) as u32;

        let mut rewards_rng = crate::rng::Rng::new(rewards_seed, 0);
        let mut transformations_rng = crate::rng::Rng::new(transformations_seed, 0);
        let mut niche_rng = crate::rng::Rng::new(niche_seed, 0);
        let mut neow_rng = crate::rng::Rng::new(neow_seed, 0);

        // Basic card transform: 82-card C/U/R pool (GetFilteredTransformationOptions strips Basic+Ancient)
        // EndOfDays at idx 25 in the 82-card pool
        let basic_pool = 82i32;
        let eod_in_pool = 25i32;
        let rewards_pick = rewards_rng.next_item(25);
        let transform_s_pick = transformations_rng.next_item(basic_pool);
        let transform_d_pick = transformations_rng.next_item(basic_pool);
        let niche_pick = niche_rng.next_item(basic_pool);
        let curse_idx = neow_rng.next_item(6);
        let toughness_or_safety = neow_rng.next_bool();
        let patience_or_scavenger = neow_rng.next_bool(); // (only if not LargeCapsule; curse=5 Empower so yes)
        // list_size = 9 (no removal, curse=Empower) + 1 + 1 = 11
        let mut positions: Vec<i32> = (0..11i32).collect();
        unstable_shuffle(&mut neow_rng, &mut positions);

        eprintln!("=== QA4AUQ9 Neow trace (basic_pool=82, EndOfDays=25) ===");
        eprintln!("rewards pick (0-24, EndOfDays=5):       {}", rewards_pick);
        eprintln!("transform_s pick (0-81, EndOfDays=25):  {} → eod={}", transform_s_pick, transform_s_pick == eod_in_pool);
        eprintln!("transform_d pick (0-81, EndOfDays=25):  {} → eod={}", transform_d_pick, transform_d_pick == eod_in_pool);
        eprintln!("niche pick (0-81, EndOfDays=25):        {} → eod={}", niche_pick, niche_pick == eod_in_pool);
        eprintln!("curse_idx: {} (0=CursedPearl,1=LargeCap,2=LeafyPoultice,3=Shears,4=Bundle,5=Empower)", curse_idx);
        eprintln!("toughness_or_safety: {}", toughness_or_safety);
        eprintln!("patience_or_scavenger: {}", patience_or_scavenger);
        eprintln!("shuffled positions: {:?}", positions);
        eprintln!("offered[0]: {} offered[1]: {} (5=NewLeaf)", positions[0], positions[1]);
        eprintln!("NewLeaf offered: {}", positions[0] == 5 || positions[1] == 5);
    }

    #[test]
    fn trace_2ef5uns_neow() {
        let str_seed = "2EF5UNS";
        let net_id = 1u64;
        let run_seed = make_run_seed(str_seed);
        let player_seed = run_seed.wrapping_add(net_id as u32);
        let rewards_seed = make_rng_seed(player_seed, "rewards");
        let transformations_seed = make_rng_seed(player_seed, "transformations");
        let niche_seed = make_rng_seed(run_seed, "niche");
        let neow_seed = (run_seed as u64)
            .wrapping_add(net_id)
            .wrapping_add(get_deterministic_hash_code("NEOW") as i64 as u64) as u32;

        let mut rewards_rng = crate::rng::Rng::new(rewards_seed, 0);
        let mut transformations_rng = crate::rng::Rng::new(transformations_seed, 0);
        let mut niche_rng = crate::rng::Rng::new(niche_seed, 0);
        let mut neow_rng = crate::rng::Rng::new(neow_seed, 0);

        let basic_pool = 82i32;
        let eod_in_pool = 25i32;
        let rewards_pick = rewards_rng.next_item(25);
        let transform_s_pick = transformations_rng.next_item(basic_pool);
        let transform_d_pick = transformations_rng.next_item(basic_pool);
        let niche_pick = niche_rng.next_item(basic_pool);
        let curse_idx = neow_rng.next_item(6);
        let toughness_or_safety = neow_rng.next_bool();
        let is_large_cap = curse_idx == 1;
        let has_removal = curse_idx == 0 || curse_idx == 2 || curse_idx == 3;
        let mut list_size = 9 - i32::from(has_removal) + 1; // +toughness_or_safety
        if !is_large_cap {
            neow_rng.next_bool();
            list_size += 1;
        }
        let mut positions: Vec<i32> = (0..list_size).collect();
        unstable_shuffle(&mut neow_rng, &mut positions);
        eprintln!("=== 2EF5UNS Neow trace (basic_pool=82, EndOfDays=25) ===");
        eprintln!("rewards pick (0-24, EndOfDays=5):       {}", rewards_pick);
        eprintln!("transform_s pick (0-81, EndOfDays=25):  {} → eod={}", transform_s_pick, transform_s_pick == eod_in_pool);
        eprintln!("transform_d pick (0-81, EndOfDays=25):  {} → eod={}", transform_d_pick, transform_d_pick == eod_in_pool);
        eprintln!("niche pick (0-81, EndOfDays=25):        {} → eod={}", niche_pick, niche_pick == eod_in_pool);
        eprintln!("curse_idx: {} (0=CursedPearl,1=LargeCap,2=LeafyPoultice,3=Shears,4=Bundle,5=Empower)", curse_idx);
        eprintln!("toughness_or_safety: {}", toughness_or_safety);
        eprintln!("offered[0]: {} offered[1]: {} (0=ArcaneScroll, 5=NewLeaf, 4=NewLeaf-if-CursedPearl)", positions[0], positions[1]);
        eprintln!("new_leaf_idx = {}", if is_large_cap || has_removal && curse_idx == 2 { "removed" } else if curse_idx == 0 { "4 (shifted)" } else { "5" });
    }

    #[test]
    fn trace_pa69saq_neow() {
        let str_seed = "PA69SAQ";
        let net_id = 1u64;
        let run_seed = make_run_seed(str_seed);
        let player_seed = run_seed.wrapping_add(net_id as u32);
        let rewards_seed = make_rng_seed(player_seed, "rewards");
        let transformations_seed = make_rng_seed(player_seed, "transformations");
        let niche_seed = make_rng_seed(run_seed, "niche");
        let neow_seed = (run_seed as u64)
            .wrapping_add(net_id)
            .wrapping_add(get_deterministic_hash_code("NEOW") as i64 as u64) as u32;

        let mut rewards_rng = crate::rng::Rng::new(rewards_seed, 0);
        let mut transformations_rng = crate::rng::Rng::new(transformations_seed, 0);
        let mut niche_rng = crate::rng::Rng::new(niche_seed, 0);
        let mut neow_rng = crate::rng::Rng::new(neow_seed, 0);

        let basic_pool = 82i32;
        let eod_in_pool = 25i32;
        let rewards_pick = rewards_rng.next_item(25);
        let transform_s_pick = transformations_rng.next_item(basic_pool);
        let transform_d_pick = transformations_rng.next_item(basic_pool);
        let niche_pick = niche_rng.next_item(basic_pool);
        let curse_idx = neow_rng.next_item(6);
        let toughness_or_safety = neow_rng.next_bool();
        let is_large_cap = curse_idx == 1;
        let has_removal = curse_idx == 0 || curse_idx == 2 || curse_idx == 3;
        let mut list_size = 9 - i32::from(has_removal) + 1; // +toughness_or_safety
        if !is_large_cap {
            neow_rng.next_bool();
            list_size += 1;
        }
        let mut positions: Vec<i32> = (0..list_size).collect();
        unstable_shuffle(&mut neow_rng, &mut positions);
        eprintln!("=== PA69SAQ Neow trace (basic_pool=82, EndOfDays=25) ===");
        eprintln!("rewards pick (0-24, EndOfDays=5):       {}", rewards_pick);
        eprintln!("transform_s pick (0-81, EndOfDays=25):  {} → eod={}", transform_s_pick, transform_s_pick == eod_in_pool);
        eprintln!("transform_d pick (0-81, EndOfDays=25):  {} → eod={}", transform_d_pick, transform_d_pick == eod_in_pool);
        eprintln!("niche pick (0-81, EndOfDays=25):        {} → eod={}", niche_pick, niche_pick == eod_in_pool);
        eprintln!("curse_idx: {} (0=CursedPearl,1=LargeCap,2=LeafyPoultice,3=Shears,4=Bundle,5=Empower)", curse_idx);
        eprintln!("toughness_or_safety: {}", toughness_or_safety);
        eprintln!("offered[0]: {} offered[1]: {} (0=ArcaneScroll, 5=NewLeaf, 4=NewLeaf-if-CursedPearl)", positions[0], positions[1]);
    }

    #[test]
    fn compare_qa4auq9_vs_pa69saq() {
        fn run(s: &str) -> (u8, bool, i32, i32, i32, i32, &'static str) {
            simulate_seed_doll(s, 1, true, true, true, 4, 2, 10, 6, 25, 5, 82, 25, true, false, 3, 0, false, 5, 5, 0, -1, -1, false, QUEEN_BOSS_IDX)
        }
        for seed in ["QA4AUQ9", "PA69SAQ", "2EF5UNS"] {
            let (stage, ud, dm, db, doll, refl, neow) = run(seed);
            eprintln!("{}: stage={} underdocks={} dm_dist={} db_pos={} doll_pos={} refl_pos={} neow={}", seed, stage, ud, dm, db, doll, refl, neow);
        }
    }

    #[test]
    fn niche_rng_pool_sizes() {
        // Compare pool sizes: 82 is the correct C/U/R pool (88 - 4 Basic - 1 Ancient - 1 MultiplayerOnly).
        const MBIG: i32 = i32::MAX;
        for str_seed in ["QA4AUQ9", "PA69SAQ", "2EF5UNS"] {
            let run_seed = make_run_seed(str_seed);
            let niche_seed = make_rng_seed(run_seed, "niche");
            // Get the raw internal_sample value (first call)
            let mut dbg_rng = crate::rng::Rng::new(niche_seed, 0);
            let raw = dbg_rng.debug_internal_sample();
            let sample = raw as f64 * (1.0 / MBIG as f64);
            eprintln!("{}: niche_seed={:#010x} raw_sample={} sample={:.20} sample*82={:.20} sample*87={:.20} pick82={} pick87={}",
                str_seed,
                niche_seed,
                raw,
                sample,
                sample * 82.0,
                sample * 87.0,
                (sample * 82.0) as i32,
                (sample * 87.0) as i32,
            );
        }
    }

    // 2ULUAAX: fails cond 7 (Reflections pos=0, consumed by ancient room)
    #[test]
    fn seed_2uluaax_fails_reflections() {
        let (stage, ..) = check("2ULUAAX", 3, 5, 3);
        assert_eq!(stage, 7, "2ULUAAX should fail at cond 7 (Reflections pos check)");
    }

    // LSFARDS: the game shows Darv in Act3, not Tanx — this was a false positive before the
    // cross-pool last_idx fix.  After the fix, stage must be 9 (Act3 ancient ≠ Tanx).
    // We also manually trace the UpFront RNG to confirm Act3 ancient = Darv (idx 3).
    //
    // Uses the real default parameters from main.rs (transform_pool_size=80, not the
    // outdated 82 in the `check` helper).
    #[test]
    fn seed_lsfards_is_darv_not_tanx() {
        // Confirm the full sim fails at condition 9 (loosen doll/reflect/shops so 4-8 pass,
        // and the sim reaches the Act3 ancient check).
        let (stage, ..) = simulate_seed_doll(
            "LSFARDS", 1, true, true, true, 4, 2, 10, 6, 25, 5, 80, 25,
            true, false, 99, 0, false, 28, 25, 0, -1, -1, false, QUEEN_BOSS_IDX,
        );
        assert_eq!(stage, 9, "LSFARDS must fail cond 9 (Act3 ≠ Tanx) after the fix");

        // Manually trace UpFront RNG to determine the actual Act3 ancient index.
        let run_seed  = make_run_seed("LSFARDS");
        let underdocks = is_underdocks_from_hash(run_seed, true, false);
        assert!(underdocks, "LSFARDS should be Underdocks");

        let up_front_seed = make_rng_seed(run_seed, "up_front");
        let mut rng = crate::rng::Rng::new(up_front_seed, 230);

        // Shared Darv distribution (Acts 2 and 3)
        let act2_shared = rng.next_int(2); // NextInt(shared_count+1) where shared_count=1
        let act3_shared = rng.next_int(2 - act2_shared); // NextInt(remaining+1)

        // Act1: Underdocks
        crate::sim::track_shuffle(&mut rng, UDK.events, DROWNING_BEACON_UDK_IDX);
        crate::sim::sim_enc(&mut rng, &UDK);
        rng.next_item(1); // neow_epoch=true → 1-element list → NextInt(0,1)

        // Act2
        crate::sim::track_shuffle(&mut rng, HIVE.events, DOLL_ROOM_HIVE_IDX);
        crate::sim::sim_enc_weak::<2>(&mut rng, &HIVE);
        let act2_ancient_base = 3i32; // orobas_epoch=true
        let act2_ancient_idx = rng.next_item(act2_ancient_base + act2_shared);
        eprintln!("LSFARDS: act2_shared={} act3_shared={} act2_ancient_idx={}", act2_shared, act3_shared, act2_ancient_idx);

        // Act3
        crate::sim::track_shuffle(&mut rng, GLORY.events, REFLECTIONS_GLORY_IDX);
        crate::sim::sim_enc(&mut rng, &GLORY);
        let act3_ancient_idx = rng.next_item(3 + act3_shared);
        eprintln!("LSFARDS: act3_ancient_idx={} (pool size={}, 0=Nonupeipe,1=Tanx,2=Vakuu,3=Darv)",
            act3_ancient_idx, 3 + act3_shared);

        // Darv is always the last element: idx = 3 when act3_shared=1, or idx = 2 when act3_shared=0
        // but Vakuu is idx 2; Darv only appears when act3_shared=1.
        // Verify it is specifically Darv (index 3) with act3_shared=1.
        assert_eq!(act3_shared, 1, "LSFARDS: Darv must be in Act3 shared pool");
        assert_eq!(act3_ancient_idx, 3, "LSFARDS: Act3 ancient must be Darv (idx 3)");
    }
}
