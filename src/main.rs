mod pools;
mod rng;
mod seed;
mod sim;
mod search;

use search::{SearchConfig, search_doll_seeds};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // Global config defaults (fully unlocked solo Necrobinder, all epochs)
    let mut darv_epoch = true;
    let mut orobas_epoch = true;
    let mut neow_epoch = true;
    let mut num_other_chars: i32 = 4;      // 5 chars total, 1 excluded → 4 for Orobas SeaGlass
    let mut orobas_pool3_count: i32 = 2;   // TouchOfOrobas + ArchaicTooth
    let mut tanx_pool_size: i32 = 10;      // TriBoomerang included
    let mut neow_curse_list_size: i32 = 6; // 4 curses + Bundle + Empower (solo, canBundle=true)
    let mut rare_card_count: i32 = 25;          // 26 Rare cards - 1 MultiplayerOnly
    let mut end_of_days_rare_idx: i32 = 5;      // EndOfDays at index 5 in 25-card pool
    let mut transform_pool_size: i32 = 81;       // NecrobinderCardPool C/U/R - GlimpseBeyond (solo)
    let mut end_of_days_transform_idx: i32 = 25; // EndOfDays at index 25 in 81-card transform pool
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
    let mut num_threads: usize = 10;
    let mut start_seed: u64 = 0;

    let mut i = 1usize;
    while i < args.len() {
        match args[i].as_str() {
            "--no-darv"           => darv_epoch = false,
            "--no-orobas"         => orobas_epoch = false,
            "--no-neow"           => neow_epoch = false,
            "--no-underdocks"     => underdocks_revealed = false,
            "--always-underdocks" => always_underdocks = true,
            "--hopper-second"     => hopper_second = true,
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
                    "--drowning-pos" => drowning_beacon_max_pos = val.parse().unwrap(),
                    "--threads"      => num_threads = val.parse().unwrap(),
                    "--start-seed"   => start_seed = val.parse().unwrap(),
                    _ => {}
                }
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }

    let act1_desc = if !underdocks_revealed {
        "Overgrowth only"
    } else if always_underdocks {
        "Underdocks always"
    } else {
        "Underdocks/Overgrowth 50-50 per seed"
    };

    eprintln!("Config: darvEpoch={darv_epoch}, orobasEpoch={orobas_epoch}, neowEpoch={neow_epoch}, netId={net_id}");
    eprintln!("        numOtherChars={num_other_chars}, pool3Count={orobas_pool3_count}, tanxPool={tanx_pool_size}");
    eprintln!("        neowCurseList={neow_curse_list_size}, rareCardCount={rare_card_count}, endOfDaysRareIdx={end_of_days_rare_idx}");
    eprintln!("        transformPool={transform_pool_size}, endOfDaysTransformIdx={end_of_days_transform_idx}");
    eprintln!("Mode: doll — all constraints combined");
    eprintln!("  Ancient: Orobas(ElectricShrymp) + Tanx(ThrowingAxe) + Neow(ArcaneScroll→EndOfDays)");
    eprintln!("  Act1: {act1_desc}");
    eprintln!("  DollRoom max queue pos: {doll_room_max_pos} (fires within event room #{})", doll_room_max_pos + 1);
    eprintln!("  Reflections max queue pos: {reflections_max_pos}");
    eprintln!("  Max Act1 shops to route: {max_act1_shops}");
    if drowning_beacon_max_pos > 0 {
        eprintln!("  DrowningBeacon: within first {drowning_beacon_max_pos} playable Act1 events");
    }
    if hopper_second {
        eprintln!("  ThievingHopper: must be 2nd weak encounter in Act2");
    }
    eprintln!("  Want: {want_count} results");
    eprintln!("  Threads: {num_threads}");
    if start_seed > 0 {
        eprintln!("  Starting from seed: {start_seed}");
    }

    search_doll_seeds(&SearchConfig {
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
        want_count,
        num_threads,
        start_seed,
    });
}
