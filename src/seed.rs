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
    if !underdocks_revealed {
        return false;
    }
    if always_underdocks {
        return true;
    }
    let act_seed = get_deterministic_hash_code(str_seed) as u32;
    let mut act_rng = Rng::new(act_seed, 0);
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
//   Basic card transforms use the full solo pool without removing the original card.
//   The basic card is NOT in the C/U/R-filtered pool, so the Id filter removes nothing.
//   Full pool: 88 total - 1 GlimpseBeyond (MultiplayerOnly) = 87 solo cards = transform_pool_size + 6.
//   EndOfDays position in the 87-card pool: eod_transform_idx + 2 (2 basic cards precede it).
//   This position is the same regardless of which basic card is being transformed.
//   LeafyPoultice: both Strike and Defend transforms use the same 87-card pool → EndOfDays at eod+2.
//   NewLeaf: player picks any deck card; any basic card transformed gives EndOfDays if niche == eod+2.
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
    // Basic-card transform pool: 87 cards (full solo pool; original basic card not in C/U/R pool → not removed).
    // EndOfDays is at eod_transform_idx + 2 in the 87-card pool for all basic transforms.
    let basic_pool        = transform_pool_size + 6;
    let eod_in_basic_pool = eod_transform_idx + 2;

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
    // The pool is always 87 cards regardless of which card is chosen, so any basic works.
    if let Some(nl_idx) = new_leaf_idx {
        let new_leaf_offered = positions[0] == nl_idx || positions[1] == nl_idx;
        if new_leaf_offered && niche_eod {
            return Some("NewLeaf");
        }
    }

    None
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
// STAGE_FULL = 11 means all conditions passed.
pub const NUM_CONDITIONS: usize = 11;
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
    "Act3 boss = Queen",              // 8
    "Act3 ancient = Tanx",            // 9
    // Deferred expensive check
    "DollysMirror ≤ max shops",       // 10
];

/// Returns (stage, underdocks, dm_dist, drowning_beacon_pos, doll_pos, refl_pos, neow_path).
/// stage = number of conditions passed (0–11). STAGE_FULL (11) = full match.
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
) -> (u8, bool, i32, i32, i32, i32, &'static str) {
    let fail = |stage| (stage, false, 0, -1, 0, 0, "");

    let run_seed = make_run_seed(str_seed);

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
    let Some(neow_path) = neow_gives_end_of_days(
        &mut neow_rng, &mut rewards_rng, &mut transformations_rng, &mut niche_rng,
        neow_curse_list_size, rare_card_count, end_of_days_rare_idx,
        transform_pool_size, end_of_days_transform_idx,
    ) else {
        return fail(2);
    };

    // UpFront RNG — counter starts at 230 (relic-bag init consumed calls 0..229)
    let underdocks = is_underdocks_act1(str_seed, underdocks_revealed, always_underdocks);
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

    let act3_boss_idx = sim_enc(&mut rng, &GLORY);

    // --- Condition 8: Act3 boss = Queen ---
    if act3_boss_idx != QUEEN_BOSS_IDX {
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

    (STAGE_FULL, underdocks, dm_dist, drowning_beacon_pos, doll_room_pos, reflections_pos, neow_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(seed: &str, doll_pos: i32, refl_pos: i32, shops: i32) -> (u8, bool, i32, i32, i32, i32, &'static str) {
        simulate_seed_doll(
            seed, 1, true, true, true, 4, 2, 10, 6, 25, 5, 81, 25,
            true, false, shops, 0, false, doll_pos, refl_pos,
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

        // Basic card transform: full 87-card pool (original basic not in C/U/R pool → not removed)
        // EndOfDays at idx 27 in the 87-card pool (eod_transform_idx=25 + 2 basic cards before it)
        let basic_pool = 87i32;
        let eod_in_pool = 27i32;
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

        eprintln!("=== QA4AUQ9 Neow trace (basic_pool=87, EndOfDays=27) ===");
        eprintln!("rewards pick (0-24, EndOfDays=5):       {}", rewards_pick);
        eprintln!("transform_s pick (0-86, EndOfDays=27):  {} → eod={}", transform_s_pick, transform_s_pick == eod_in_pool);
        eprintln!("transform_d pick (0-86, EndOfDays=27):  {} → eod={}", transform_d_pick, transform_d_pick == eod_in_pool);
        eprintln!("niche pick (0-86, EndOfDays=27):        {} → eod={}", niche_pick, niche_pick == eod_in_pool);
        eprintln!("curse_idx: {} (0=CursedPearl,1=LargeCap,2=LeafyPoultice,3=Shears,4=Bundle,5=Empower)", curse_idx);
        eprintln!("toughness_or_safety: {}", toughness_or_safety);
        eprintln!("patience_or_scavenger: {}", patience_or_scavenger);
        eprintln!("shuffled positions: {:?}", positions);
        eprintln!("offered[0]: {} offered[1]: {} (5=NewLeaf)", positions[0], positions[1]);
        eprintln!("NewLeaf offered: {}", positions[0] == 5 || positions[1] == 5);
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

        let basic_pool = 87i32;
        let eod_in_pool = 27i32;
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
        eprintln!("=== PA69SAQ Neow trace (basic_pool=87, EndOfDays=27) ===");
        eprintln!("rewards pick (0-24, EndOfDays=5):       {}", rewards_pick);
        eprintln!("transform_s pick (0-86, EndOfDays=27):  {} → eod={}", transform_s_pick, transform_s_pick == eod_in_pool);
        eprintln!("transform_d pick (0-86, EndOfDays=27):  {} → eod={}", transform_d_pick, transform_d_pick == eod_in_pool);
        eprintln!("niche pick (0-86, EndOfDays=27):        {} → eod={}", niche_pick, niche_pick == eod_in_pool);
        eprintln!("curse_idx: {} (0=CursedPearl,1=LargeCap,2=LeafyPoultice,3=Shears,4=Bundle,5=Empower)", curse_idx);
        eprintln!("toughness_or_safety: {}", toughness_or_safety);
        eprintln!("offered[0]: {} offered[1]: {} (0=ArcaneScroll, 5=NewLeaf, 4=NewLeaf-if-CursedPearl)", positions[0], positions[1]);
    }

    #[test]
    fn compare_qa4auq9_vs_pa69saq() {
        fn run(s: &str) -> (u8, bool, i32, i32, i32, i32, &'static str) {
            simulate_seed_doll(s, 1, true, true, true, 4, 2, 10, 6, 25, 5, 81, 25, true, false, 3, 0, false, 5, 5)
        }
        for seed in ["QA4AUQ9", "PA69SAQ"] {
            let (stage, ud, dm, db, doll, refl, neow) = run(seed);
            eprintln!("{}: stage={} underdocks={} dm_dist={} db_pos={} doll_pos={} refl_pos={} neow={}", seed, stage, ud, dm, db, doll, refl, neow);
        }
    }

    #[test]
    fn niche_rng_pool_sizes() {
        for str_seed in ["QA4AUQ9", "PA69SAQ"] {
            let run_seed = make_run_seed(str_seed);
            let niche_seed = make_rng_seed(run_seed, "niche");
            let mut rng81 = crate::rng::Rng::new(niche_seed, 0);
            let mut rng86 = crate::rng::Rng::new(niche_seed, 0);
            let mut rng87 = crate::rng::Rng::new(niche_seed, 0);
            eprintln!("{}: niche[0](81)={} niche[0](86)={} niche[0](87)={}",
                str_seed,
                rng81.next_item(81),
                rng86.next_item(86),
                rng87.next_item(87));
        }
    }

    // 2ULUAAX: fails cond 7 (Reflections pos=0, consumed by ancient room)
    #[test]
    fn seed_2uluaax_fails_reflections() {
        let (stage, ..) = check("2ULUAAX", 3, 5, 3);
        assert_eq!(stage, 7, "2ULUAAX should fail at cond 7 (Reflections pos check)");
    }
}
