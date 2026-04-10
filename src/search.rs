use rayon::prelude::*;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};
use std::sync::Mutex;

use crate::pools::{GLORY, HIVE, UDK};
use crate::seed::{
    int_to_seed_string, simulate_seed_doll, CONDITION_NAMES, NUM_CONDITIONS, STAGE_FULL,
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

fn print_stats(stats: &Stats, cfg: &SearchConfig, total: u64) {
    eprintln!("--- Progress: {} seeds examined ---", total);
    for (i, name) in CONDITION_NAMES.iter().enumerate() {
        let passed  = stats.passed[i].load(Ordering::Relaxed);
        // reached[i] = seeds that actually evaluated condition i
        //   = passed[i-1] for i>0, or total for i==0
        let reached = if i == 0 { total } else { stats.passed[i - 1].load(Ordering::Relaxed) };
        let na = match i {
            4 => cfg.drowning_beacon_max_pos == 0,
            6 => !cfg.hopper_second,
            _ => false,
        };
        if na {
            eprintln!("  [{i:2}] {name}: N/A");
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
                );

                // Accumulate per-condition stats (cumulative: cond[i] means passed 0..=i)
                for i in 0..(stage as usize).min(NUM_CONDITIONS) {
                    stats.passed[i].fetch_add(1, Ordering::Relaxed);
                }

                // Track total examined and print every 100k
                let prev = stats.examined.fetch_add(1, Ordering::Relaxed);
                let new_total = prev + 1;
                if prev / 100_000_000 != new_total / 100_000_000 {
                    print_stats(&stats, cfg, new_total);
                }

                if stage == STAGE_FULL {
                    let count = found.fetch_add(1, Ordering::Relaxed) + 1;
                    if count > cfg.want_count {
                        break;
                    }
                    let act1_name = if underdocks { "Underdocks" } else { "Overgrowth" };
                    let dm_msg = if dm_dist == 0 {
                        "skip all Act1 shops → appears in first Act2 shop".to_string()
                    } else {
                        format!("visit exactly {dm_dist} Act1 shop(s) → appears in first Act2 shop")
                    };
                    let _guard = out_lock.lock().unwrap();
                    println!("Seed: {str_seed}  (Act1={act1_name})");
                    println!("  Act2 ancient:   Orobas (offering ElectricShrymp)");
                    println!("  Act3 ancient:   Tanx (offering ThrowingAxe)");
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
                    println!();
                }
            }
        });
    });

    // Final stats
    let total = stats.examined.load(Ordering::Relaxed);
    eprintln!("--- Final stats: {} seeds examined ---", total);
    print_stats(&stats, cfg, total.max(1));
}
