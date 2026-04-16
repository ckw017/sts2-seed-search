mod map;
mod pools;
mod rng;
mod seed;
mod sim;
mod search;

use search::{SearchConfig, load_target_hashes, search_doll_seeds, search_doll_seeds_by_hash, search_seeds_by_target_hashes};
use seed::{DEFECT_VOLTAIC_RARE_COUNT, DEFECT_VOLTAIC_RARE_IDX, DEFECT_VOLTAIC_TRANSFORM_IDX};
use pools::{QUEEN_BOSS_IDX, TEST_SUBJECT_BOSS_IDX};

fn parse_usize(args: &[String], flag: &str) -> Option<usize> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].parse().unwrap())
}

fn parse_u64(args: &[String], flag: &str) -> Option<u64> {
    args.windows(2).find(|w| w[0] == flag).map(|w| w[1].parse().unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // find-seeds mode: independent of all doll-seed config
    if let Some(pos) = args.iter().position(|a| a == "--find-seeds") {
        let path = args.get(pos + 1).expect("--find-seeds requires a file argument");
        let num_threads = parse_usize(&args, "--threads").unwrap_or(10);
        let start_seed  = parse_u64(&args, "--start-seed").unwrap_or(0);
        let targets = load_target_hashes(path);
        eprintln!("find-seeds: {} targets from {path}, threads={num_threads}", targets.len());
        if start_seed > 0 {
            eprintln!("  Starting from seed: {start_seed}");
        }
        search_seeds_by_target_hashes(&targets, num_threads, start_seed);
        return;
    }

    // Detect mode early to set character-specific defaults
    let defect_voltaic_mode = args.iter().any(|a| a == "--defect-voltaic");

    // Global config defaults (fully unlocked solo Necrobinder, all epochs)
    let mut darv_epoch = true;
    let mut orobas_epoch = true;
    let mut neow_epoch = true;
    let mut num_other_chars: i32 = 4;      // 5 chars total, 1 excluded → 4 for Orobas SeaGlass
    let mut orobas_pool3_count: i32 = 2;   // TouchOfOrobas + ArchaicTooth
    let mut tanx_pool_size: i32 = 10;      // TriBoomerang included
    let mut neow_curse_list_size: i32 = 6; // 4 curses + Bundle + Empower (solo, canBundle=true)
    // Card pool defaults differ by character/mode:
    let mut rare_card_count: i32;
    let mut end_of_days_rare_idx: i32;
    let mut transform_pool_size: i32 = 80;
    let mut end_of_days_transform_idx: i32;
    if defect_voltaic_mode {
        // Fully unlocked solo Defect: 25 Rare cards, Voltaic at index 24; transform pool 80, Voltaic at 78
        rare_card_count          = DEFECT_VOLTAIC_RARE_COUNT;
        end_of_days_rare_idx     = DEFECT_VOLTAIC_RARE_IDX;
        end_of_days_transform_idx = DEFECT_VOLTAIC_TRANSFORM_IDX;
    } else {
        // Necrobinder defaults: 25 Rare cards, EndOfDays at index 5; transform pool 80, EndOfDays at 25
        rare_card_count          = 25;  // 26 Rare cards - 1 MultiplayerOnly
        end_of_days_rare_idx     = 5;   // EndOfDays at index 5 in 25-card pool
        end_of_days_transform_idx = 25; // EndOfDays at index 25 in 80-card transform pool
    }
    let mut net_id: u64 = 1;

    // Search/mode config defaults
    let mut want_count: i32 = 10;
    let mut doll_room_max_pos: i32 = 1;    // DollRoom at queue pos ≤ 1 (fires on 2nd event room)
    let mut reflections_max_pos: i32 = 2;  // Reflections at queue pos ≤ 2
    let mut max_act1_shops: i32 = 1;
    let mut underdocks_revealed = true;
    let mut always_underdocks = false;
    let mut drowning_beacon_max_pos: i32 = 0; // 0 = no constraint
    let mut hopper_second = false;
    let mut lamp_treasure_tries: i32 = 0;
    let mut lamp_elite_pos_max: i32 = -1;  // -1 = disabled; pos from front (0 = first elite draw)
    let mut lamp_shop_pos_max: i32 = -1;   // -1 = disabled; pos from back  (0 = first shop draw)
    let mut no_lamp = false;
    let mut act3_boss_idx: i32 = QUEEN_BOSS_IDX; // default: Queen boss required
    let mut num_threads: usize = 10;
    let mut start_seed: u64 = 0;
    let mut hash_mode = false;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--hash-mode"         => hash_mode = true,
            "--no-darv"           => darv_epoch = false,
            "--no-orobas"         => orobas_epoch = false,
            "--no-neow"           => neow_epoch = false,
            "--no-underdocks"     => underdocks_revealed = false,
            "--always-underdocks" => always_underdocks = true,
            "--hopper-second"     => hopper_second = true,
            "--no-lamp"           => no_lamp = true,
            "--test-subject"      => act3_boss_idx = TEST_SUBJECT_BOSS_IDX,
            "--any-boss"          => act3_boss_idx = -1,
            "--defect-voltaic"    => {} // handled via defect_voltaic_mode (detected before arg parse)
            flag if i + 1 < args.len() => {
                let val = &args[i + 1];
                match flag {
                    "--net-id"       => net_id = val.parse().unwrap(),
                    "--other-chars"  => num_other_chars = val.parse().unwrap(),
                    "--pool3"        => orobas_pool3_count = val.parse().unwrap(),
                    "--tanx-pool"    => tanx_pool_size = val.parse().unwrap(),
                    "--curse-list"   => neow_curse_list_size = val.parse().unwrap(),
                    "--rare-count"      => rare_card_count = val.parse().unwrap(),
                    "--eod-idx"         => end_of_days_rare_idx = val.parse().unwrap(),
                    "--transform-pool"  => transform_pool_size = val.parse().unwrap(),
                    "--eod-transform"   => end_of_days_transform_idx = val.parse().unwrap(),
                    "--count"        => want_count = val.parse().unwrap(),
                    "--doll-pos"     => doll_room_max_pos = val.parse().unwrap(),
                    "--reflect-pos"  => reflections_max_pos = val.parse().unwrap(),
                    "--max-shops"    => max_act1_shops = val.parse().unwrap(),
                    "--drowning-pos"   => drowning_beacon_max_pos = val.parse().unwrap(),
                    "--lamp-treasure"   => lamp_treasure_tries = val.parse().unwrap(),
                    "--lamp-elite-pos"  => lamp_elite_pos_max = val.parse().unwrap(),
                    "--lamp-shop-pos"   => lamp_shop_pos_max = val.parse().unwrap(),
                    "--threads"        => num_threads = val.parse().unwrap(),
                    "--start-seed"   => start_seed = val.parse().unwrap(),
                    _ => {}
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let cfg = SearchConfig {
        darv_epoch,
        orobas_epoch,
        neow_epoch,
        num_other_chars,
        orobas_pool3_count,
        tanx_pool_size,
        neow_curse_list_size,
        rare_card_count,
        end_of_days_rare_idx,
        transform_pool_size,
        end_of_days_transform_idx,
        net_id,
        doll_room_max_pos,
        reflections_max_pos,
        max_act1_shops,
        underdocks_revealed,
        always_underdocks,
        drowning_beacon_max_pos,
        hopper_second,
        lamp_treasure_tries,
        lamp_elite_pos_max,
        lamp_shop_pos_max,
        no_lamp,
        act3_boss_idx,
        neow_card_name: if defect_voltaic_mode { "Voltaic" } else { "EndOfDays" },
        want_count,
        num_threads,
        start_seed,
    };
    let act1_desc = if !cfg.underdocks_revealed {
        "Overgrowth only"
    } else if cfg.always_underdocks {
        "Underdocks always"
    } else {
        "Underdocks/Overgrowth 50-50 per seed"
    };
    let neow_target = if defect_voltaic_mode { "Voltaic" } else { "EndOfDays" };
    eprintln!("Config: darvEpoch={}, orobasEpoch={}, neowEpoch={}, netId={}", cfg.darv_epoch, cfg.orobas_epoch, cfg.neow_epoch, cfg.net_id);
    eprintln!("        numOtherChars={}, pool3Count={}, tanxPool={}", cfg.num_other_chars, cfg.orobas_pool3_count, cfg.tanx_pool_size);
    eprintln!("        neowCurseList={}, rareCardCount={}, neowCardIdx={}", cfg.neow_curse_list_size, cfg.rare_card_count, cfg.end_of_days_rare_idx);
    eprintln!("        transformPool={}, transformIdx={}", cfg.transform_pool_size, cfg.end_of_days_transform_idx);
    eprintln!("Mode: doll — all constraints combined");
    eprintln!("  Ancient: Orobas(ElectricShrymp) + Tanx(ThrowingAxe) + Neow(ArcaneScroll→{neow_target})");
    eprintln!("  Act1: {act1_desc}");
    eprintln!("  DollRoom max queue pos: {} (fires within event room #{})", cfg.doll_room_max_pos, cfg.doll_room_max_pos + 1);
    eprintln!("  Reflections max queue pos: {}", cfg.reflections_max_pos);
    eprintln!("  Max Act1 shops to route: {}", cfg.max_act1_shops);
    if cfg.drowning_beacon_max_pos > 0 {
        eprintln!("  DrowningBeacon: within first {} playable Act1 events", cfg.drowning_beacon_max_pos);
    }
    if cfg.hopper_second {
        eprintln!("  ThievingHopper: must be 2nd weak encounter in Act2");
    }
    let boss_label = match cfg.act3_boss_idx {
        -1 => "any".to_string(),
        i if i == QUEEN_BOSS_IDX        => format!("Queen (idx {i})"),
        i if i == TEST_SUBJECT_BOSS_IDX => format!("TestSubject (idx {i})"),
        i => format!("idx {i}"),
    };
    eprintln!("  Act3 boss: {boss_label}");
    if cfg.no_lamp {
        eprintln!("  UnsettlingLamp: disabled (--no-lamp)");
    } else {
        if cfg.lamp_treasure_tries > 0 {
            eprintln!("  UnsettlingLamp: within first {} treasure room visits", cfg.lamp_treasure_tries);
        }
        if cfg.lamp_elite_pos_max >= 0 {
            eprintln!("  UnsettlingLamp: elite pos ≤ {} from front", cfg.lamp_elite_pos_max);
        }
        if cfg.lamp_shop_pos_max >= 0 {
            eprintln!("  UnsettlingLamp: shop pos ≤ {} from back", cfg.lamp_shop_pos_max);
        }
    }
    if hash_mode {
        eprintln!("  Mode: hash — iterating all 2^32 run_seed values");
    }
    eprintln!("  Want: {} results", cfg.want_count);
    eprintln!("  Threads: {}", cfg.num_threads);
    if cfg.start_seed > 0 {
        eprintln!("  Starting from seed: {}", cfg.start_seed);
    }
    if hash_mode {
        search_doll_seeds_by_hash(&cfg);
    } else {
        search_doll_seeds(&cfg);
    }
}
