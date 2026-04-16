use rayon::prelude::*;
use std::collections::HashSet;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::pools::{GLORY, HIVE, PLAYER_RARE_SIZE, QUEEN_BOSS_IDX, TEST_SUBJECT_BOSS_IDX, UDK};

fn boss_name(idx: i32) -> &'static str {
    match idx {
        0 => "Doormaker",
        i if i == QUEEN_BOSS_IDX        => "Queen",
        i if i == TEST_SUBJECT_BOSS_IDX => "TestSubject",
        _ => "unknown",
    }
}
use crate::seed::{
    int_to_seed_string, make_run_seed, simulate_seed_doll, simulate_seed_doll_by_hash,
    simulate_seed_defect_voltaic,
    unsettling_lamp_positions, unsettling_lamp_treasure_visit,
    CONDITION_NAMES, NUM_CONDITIONS, STAGE_FULL,
};

pub struct SearchConfig {
    pub darv_epoch: bool,
    pub orobas_epoch: bool,
    pub neow_epoch: bool,
    pub num_other_chars: i32,
    pub orobas_pool3_count: i32,
    pub tanx_pool_size: i32,
    pub neow_curse_list_size: i32,
    pub rare_card_count: i32,
    pub end_of_days_rare_idx: i32,
    pub transform_pool_size: i32,
    pub end_of_days_transform_idx: i32,
    pub net_id: u64,
    pub doll_room_max_pos: i32,
    pub reflections_max_pos: i32,
    pub max_act1_shops: i32,
    pub underdocks_revealed: bool,
    pub always_underdocks: bool,
    pub drowning_beacon_max_pos: i32,
    pub hopper_second: bool,
    pub lamp_treasure_tries: i32,
    pub lamp_elite_pos_max: i32,  // -1 = disabled; pos from front (0 = first elite draw)
    pub lamp_shop_pos_max: i32,   // -1 = disabled; pos from back  (0 = first shop draw)
    pub no_lamp: bool,            // true = skip lamp constraint (overrides all lamp_* params)
    pub act3_boss_idx: i32,       // required Act3 boss index; -1 = any boss accepted
    pub neow_card_name: &'static str, // "EndOfDays" or "Voltaic" etc.
    pub want_count: i32,
    pub num_threads: usize,
    pub start_seed: u64,
}

struct Stats {
    examined:  AtomicU64,
    passed:    [AtomicU64; NUM_CONDITIONS],
}

impl Stats {
    fn new() -> Self {
        Self {
            examined: AtomicU64::new(0),
            passed: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }
}

fn print_stats(stats: &Stats, cfg: &SearchConfig, total: u64, label: &str) {
    if cfg.start_seed > 0 {
        eprintln!("--- {label}: {} ---", cfg.start_seed + total);
    } else {
        eprintln!("--- {label}: {} seeds examined ---", total);
    }
    for (i, name) in CONDITION_NAMES.iter().enumerate() {
        let passed  = stats.passed[i].load(Ordering::Relaxed);
        // reached[i] = seeds that actually evaluated condition i
        //   = passed[i-1] for i>0, or total for i==0
        let reached = if i == 0 { total } else { stats.passed[i - 1].load(Ordering::Relaxed) };
        let na = match i {
            3  => cfg.drowning_beacon_max_pos == 0,
            5  => !cfg.hopper_second,
            8  => cfg.act3_boss_idx < 0,
            11 => cfg.no_lamp || (cfg.lamp_treasure_tries == 0 && cfg.lamp_elite_pos_max < 0 && cfg.lamp_shop_pos_max < 0),
            _  => false,
        };
        if na {
            continue;
        } else if reached == 0 {
            eprintln!("  [{i:2}] {name}: 0/0");
        } else {
            let pct = passed as f64 / reached as f64 * 100.0;
            eprintln!("  [{i:2}] {name}: {passed}/{reached} ({pct:.4}%)");
        }
    }
}

pub fn search_doll_seeds(cfg: &SearchConfig) {
    let found   = AtomicI32::new(0);
    let stats   = Stats::new();
    let out_lock = Mutex::new(());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.num_threads)
        .build()
        .unwrap();

    pool.install(|| {
        (0..cfg.num_threads).into_par_iter().for_each(|thread_offset| {
            let mut n = cfg.start_seed + thread_offset as u64;
            while found.load(Ordering::Relaxed) < cfg.want_count {
                let str_seed = int_to_seed_string(n);
                n += cfg.num_threads as u64;

                let (stage, underdocks, dm_dist, db_pos, doll_pos, refl_pos, neow_path) = simulate_seed_doll(
                    &str_seed,
                    cfg.net_id,
                    cfg.darv_epoch,
                    cfg.orobas_epoch,
                    cfg.neow_epoch,
                    cfg.num_other_chars,
                    cfg.orobas_pool3_count,
                    cfg.tanx_pool_size,
                    cfg.neow_curse_list_size,
                    cfg.rare_card_count,
                    cfg.end_of_days_rare_idx,
                    cfg.transform_pool_size,
                    cfg.end_of_days_transform_idx,
                    cfg.underdocks_revealed,
                    cfg.always_underdocks,
                    cfg.max_act1_shops,
                    cfg.drowning_beacon_max_pos,
                    cfg.hopper_second,
                    cfg.doll_room_max_pos,
                    cfg.reflections_max_pos,
                    cfg.lamp_treasure_tries,
                    cfg.lamp_elite_pos_max,
                    cfg.lamp_shop_pos_max,
                    cfg.no_lamp,
                    cfg.act3_boss_idx,
                );

                // Accumulate per-condition stats (cumulative: cond[i] means passed 0..=i)
                for i in 0..(stage as usize).min(NUM_CONDITIONS) {
                    stats.passed[i].fetch_add(1, Ordering::Relaxed);
                }

                // Track total examined and print every 100k
                let prev = stats.examined.fetch_add(1, Ordering::Relaxed);
                let new_total = prev + 1;
                if prev / 100_000_000 != new_total / 100_000_000 {
                    print_stats(&stats, cfg, new_total, "Progress");
                }

                if stage == STAGE_FULL {
                    let count = found.fetch_add(1, Ordering::Relaxed) + 1;
                    if count > cfg.want_count {
                        break;
                    }
                    let act1_name = if underdocks { "Underdocks" } else { "Overgrowth" };
                    let dm_msg = format!("Dolly's Mirror in shop visit #{}", dm_dist + 1);
                    let boss_name = boss_name(cfg.act3_boss_idx);
                    let _guard = out_lock.lock().unwrap();
                    println!("Seed: {str_seed}  (Act1={act1_name})");
                    println!("  Act2 ancient:   Orobas (offering ElectricShrymp)");
                    println!("  Act3 ancient:   Tanx (offering ThrowingAxe)");
                    if cfg.act3_boss_idx >= 0 {
                        println!("  Act3 boss:      {boss_name}");
                    }
                    let card = cfg.neow_card_name;
                    let neow_msg = match neow_path {
                        "ArcaneScroll"  => format!("take ArcaneScroll → grants {card}"),
                        "LeafyPoultice" => format!("take LeafyPoultice curse → Strike/Defend auto-transform into {card}"),
                        "NewLeaf"       => format!("take NewLeaf → transform any basic card into {card}"),
                        _               => neow_path.to_string(),
                    };
                    println!("  Neow:           {neow_msg}");
                    if underdocks {
                        println!(
                            "  DrowningBeacon: Act1 event room #{} (queue pos {} of {})",
                            db_pos, db_pos + 1, UDK.events
                        );
                    }
                    println!(
                        "  DollRoom:       Act2 event room #{} (queue pos {} of {})",
                        doll_pos, doll_pos + 1, HIVE.events
                    );
                    println!(
                        "  Reflections:    Act3 event room #{} (queue pos {} of {})",
                        refl_pos, refl_pos + 1, GLORY.events
                    );
                    println!("  Dolly's Mirror: {dm_msg}");
                    if !cfg.no_lamp && (cfg.lamp_treasure_tries > 0 || cfg.lamp_elite_pos_max >= 0 || cfg.lamp_shop_pos_max >= 0) {
                        let run_seed = make_run_seed(&str_seed);
                        let (shared_pos, player_pos) = unsettling_lamp_positions(run_seed);
                        if cfg.lamp_treasure_tries > 0 {
                            let visit = unsettling_lamp_treasure_visit(run_seed, shared_pos);
                            println!("  UnsettlingLamp: treasure room visit #{visit}");
                        }
                        if cfg.lamp_elite_pos_max >= 0 || cfg.lamp_shop_pos_max >= 0 {
                            let shop_pos = (PLAYER_RARE_SIZE - 1) - player_pos;
                            println!("  UnsettlingLamp: elite draw #{} / shop draw #{}", player_pos + 1, shop_pos + 1);
                        }
                    }
                    println!();
                }
            }
        });
    });

    // Final stats
    let total = stats.examined.load(Ordering::Relaxed);
    print_stats(&stats, cfg, total.max(1), "Final");
}

pub fn search_doll_seeds_by_hash(cfg: &SearchConfig) {
    let found    = AtomicI32::new(0);
    let stats    = Stats::new();
    let out_lock = Mutex::new(());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.num_threads)
        .build()
        .unwrap();

    // Iterate all 2^32 possible run_seed values, starting from cfg.start_seed (as u32).
    let start = cfg.start_seed as u32;

    pool.install(|| {
        (0..cfg.num_threads).into_par_iter().for_each(|thread_offset| {
            let mut run_seed = start.wrapping_add(thread_offset as u32);
            let mut iterations: u64 = 0;
            loop {
                if found.load(Ordering::Relaxed) >= cfg.want_count {
                    break;
                }

                let (stage, underdocks, dm_dist, db_pos, doll_pos, refl_pos, neow_path) =
                    simulate_seed_doll_by_hash(
                        run_seed,
                        cfg.net_id,
                        cfg.darv_epoch,
                        cfg.orobas_epoch,
                        cfg.neow_epoch,
                        cfg.num_other_chars,
                        cfg.orobas_pool3_count,
                        cfg.tanx_pool_size,
                        cfg.neow_curse_list_size,
                        cfg.rare_card_count,
                        cfg.end_of_days_rare_idx,
                        cfg.transform_pool_size,
                        cfg.end_of_days_transform_idx,
                        cfg.underdocks_revealed,
                        cfg.always_underdocks,
                        cfg.max_act1_shops,
                        cfg.drowning_beacon_max_pos,
                        cfg.hopper_second,
                        cfg.doll_room_max_pos,
                        cfg.reflections_max_pos,
                        cfg.lamp_treasure_tries,
                        cfg.lamp_elite_pos_max,
                        cfg.lamp_shop_pos_max,
                        cfg.no_lamp,
                        cfg.act3_boss_idx,
                    );

                for i in 0..(stage as usize).min(NUM_CONDITIONS) {
                    stats.passed[i].fetch_add(1, Ordering::Relaxed);
                }

                let prev = stats.examined.fetch_add(1, Ordering::Relaxed);
                let new_total = prev + 1;
                if prev / 100_000_000 != new_total / 100_000_000 {
                    print_stats(&stats, cfg, new_total, "Progress");
                }

                if stage == STAGE_FULL {
                    let count = found.fetch_add(1, Ordering::Relaxed) + 1;
                    if count <= cfg.want_count {
                        let act1_name = if underdocks { "Underdocks" } else { "Overgrowth" };
                        let dm_msg = format!("shop visit #{}", dm_dist + 1);
                        let boss_name = boss_name(cfg.act3_boss_idx);
                        let _guard = out_lock.lock().unwrap();
                        println!("Hash: {run_seed:#010x}  (Act1={act1_name})");
                        println!("  Act2 ancient:   Orobas (offering ElectricShrymp)");
                        println!("  Act3 ancient:   Tanx (offering ThrowingAxe)");
                        if cfg.act3_boss_idx >= 0 {
                            println!("  Act3 boss:      {boss_name}");
                        }
                        let neow_msg = match neow_path {
                            "ArcaneScroll"  => "take ArcaneScroll → grants EndOfDays",
                            "LeafyPoultice" => "take LeafyPoultice curse → Strike/Defend auto-transform into EndOfDays",
                            "NewLeaf"       => "take NewLeaf → transform any basic card into EndOfDays",
                            _               => neow_path,
                        };
                        println!("  Neow:           {neow_msg}");
                        if underdocks {
                            println!(
                                "  DrowningBeacon: Act1 event room #{} (queue pos {} of {})",
                                db_pos, db_pos + 1, UDK.events
                            );
                        }
                        println!(
                            "  DollRoom:       Act2 event room #{} (queue pos {} of {})",
                            doll_pos, doll_pos + 1, HIVE.events
                        );
                        println!(
                            "  Reflections:    Act3 event room #{} (queue pos {} of {})",
                            refl_pos, refl_pos + 1, GLORY.events
                        );
                        println!("  Dolly's Mirror: {dm_msg}");
                        if !cfg.no_lamp && (cfg.lamp_treasure_tries > 0 || cfg.lamp_elite_pos_max >= 0 || cfg.lamp_shop_pos_max >= 0) {
                            let (shared_pos, player_pos) = unsettling_lamp_positions(run_seed);
                            if cfg.lamp_treasure_tries > 0 {
                                let visit = unsettling_lamp_treasure_visit(run_seed, shared_pos);
                                println!("  UnsettlingLamp: treasure room visit #{visit}");
                            }
                            if cfg.lamp_elite_pos_max >= 0 || cfg.lamp_shop_pos_max >= 0 {
                                let shop_pos = (PLAYER_RARE_SIZE - 1) - player_pos;
                                println!("  UnsettlingLamp: elite draw #{} / shop draw #{}", player_pos + 1, shop_pos + 1);
                            }
                        }
                        println!();
                    }
                }

                iterations += 1;
                // Each thread steps by num_threads; stop after covering the full u32 space.
                if iterations >= (u32::MAX as u64 / cfg.num_threads as u64 + 1) {
                    break;
                }
                run_seed = run_seed.wrapping_add(cfg.num_threads as u32);
            }
        });
    });

    let total = stats.examined.load(Ordering::Relaxed);
    print_stats(&stats, cfg, total.max(1), "Final");
}

/// Search for seeds where a solo Defect player gets Voltaic from Neow.
/// Only checks the Neow reward (ArcaneScroll, LeafyPoultice, NewLeaf paths).
pub fn search_defect_voltaic(cfg: &SearchConfig) {
    let found    = AtomicI32::new(0);
    let examined = AtomicU64::new(0);
    let out_lock = Mutex::new(());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(cfg.num_threads)
        .build()
        .unwrap();

    pool.install(|| {
        (0..cfg.num_threads).into_par_iter().for_each(|thread_offset| {
            let mut n = cfg.start_seed + thread_offset as u64;
            while found.load(Ordering::Relaxed) < cfg.want_count {
                let str_seed = int_to_seed_string(n);
                n += cfg.num_threads as u64;

                let run_seed = make_run_seed(&str_seed);
                let path = simulate_seed_defect_voltaic(
                    run_seed,
                    cfg.net_id,
                    cfg.neow_curse_list_size,
                    cfg.rare_card_count,
                    cfg.end_of_days_rare_idx,
                    cfg.transform_pool_size,
                    cfg.end_of_days_transform_idx,
                );

                let prev = examined.fetch_add(1, Ordering::Relaxed);
                let new_total = prev + 1;
                if prev / 100_000_000 != new_total / 100_000_000 {
                    eprintln!("--- Progress: {new_total} seeds examined ---");
                }

                if let Some(neow_path) = path {
                    let count = found.fetch_add(1, Ordering::Relaxed) + 1;
                    if count > cfg.want_count {
                        break;
                    }
                    let neow_msg = match neow_path {
                        "ArcaneScroll"  => "take ArcaneScroll → grants Voltaic",
                        "LeafyPoultice" => "take LeafyPoultice curse → Strike/Defend auto-transform into Voltaic",
                        "NewLeaf"       => "take NewLeaf → transform any basic card into Voltaic",
                        _               => neow_path,
                    };
                    let _guard = out_lock.lock().unwrap();
                    println!("Seed: {str_seed}");
                    println!("  Neow: {neow_msg}");
                    println!();
                }
            }
        });
    });

    let total = examined.load(Ordering::Relaxed);
    eprintln!("--- Final: {} seeds examined ---", total.max(1));
}

/// Read target hashes from a file (one per line, hex with or without 0x prefix).
pub fn load_target_hashes(path: &str) -> HashSet<u32> {
    std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {path}: {e}"))
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let s = l.trim().trim_start_matches("0x").trim_start_matches("0X");
            u32::from_str_radix(s, 16).unwrap_or_else(|e| panic!("bad hash {l:?}: {e}"))
        })
        .collect()
}

/// Search for string seeds whose run_seed (deterministic hash) matches any target hash.
/// Iterates base-34 strings in order; stops once all targets are matched.
pub fn search_seeds_by_target_hashes(targets: &HashSet<u32>, num_threads: usize, start_seed: u64) {
    let remaining = AtomicI32::new(targets.len() as i32);
    let examined  = AtomicU64::new(0);
    let out_lock  = Mutex::new(());
    let found_set = Mutex::new(HashSet::<u32>::new());

    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build()
        .unwrap();

    pool.install(|| {
        (0..num_threads).into_par_iter().for_each(|thread_offset| {
            let mut n = start_seed + thread_offset as u64;
            loop {
                if remaining.load(Ordering::Relaxed) <= 0 {
                    break;
                }
                let str_seed = int_to_seed_string(n);
                let hash = make_run_seed(&str_seed);
                if targets.contains(&hash) {
                    let mut set = found_set.lock().unwrap();
                    if set.insert(hash) {
                        remaining.fetch_sub(1, Ordering::Relaxed);
                        let _guard = out_lock.lock().unwrap();
                        println!("{str_seed}  (hash {hash:#010x})");
                    }
                }
                let prev = examined.fetch_add(1, Ordering::Relaxed);
                let total = prev + 1;
                if prev / 100_000_000 != total / 100_000_000 {
                    let left = remaining.load(Ordering::Relaxed);
                    eprintln!("--- {total} seeds examined, {left} hashes remaining ---");
                }
                n += num_threads as u64;
            }
        });
    });
}
