use crate::pools::{ActPools, Enc};
use crate::rng::Rng;

// Fisher-Yates unstable shuffle (matches C# Rng.UnstableShuffle)
pub fn unstable_shuffle(rng: &mut Rng, arr: &mut [i32]) {
    let mut num = arr.len();
    while num > 1 {
        num -= 1;
        let j = rng.next_int(num as i32 + 1) as usize;
        arr.swap(j, num);
    }
}

// Fisher-Yates tracking where `target` element ends up (matches C# TrackShuffle)
pub fn track_shuffle(rng: &mut Rng, n: i32, target: i32) -> i32 {
    let mut pos = target;
    let mut num = n;
    while num > 1 {
        num -= 1;
        let j = rng.next_int(num + 1);
        if pos == num {
            pos = j;
        } else if pos == j {
            pos = num;
        }
    }
    pos
}

// Advance rng by the number of NextInt calls a Fisher-Yates of `deque_size` elements uses.
pub fn advance_shuffle(rng: &mut Rng, deque_size: i32) {
    let mut i = deque_size - 1;
    while i > 0 {
        rng.next_int(i + 1);
        i -= 1;
    }
}

// ---------------------------------------------------------------------------
// GrabBag simulation
//
// Matches C# GrabBagPick: AddWithoutRepeatingTags predicate is
//   !SharesTagsWith(last) && idx != lastIdx
// If nothing in bag satisfies predicate: one unconditional NextDouble call.
// Otherwise: loop NextDouble calls until a valid item is hit.
// ---------------------------------------------------------------------------

#[inline]
fn can_pick(pool: &[Enc], idx: i32, last_idx: i32, last_mask: u16) -> bool {
    // last_mask=0 means no last (always passes tag check)
    idx != last_idx && (pool[idx as usize].tags & last_mask == 0)
}

pub fn grab_bag_pick(
    rng: &mut Rng,
    bag: &mut Vec<i32>,
    pool: &[Enc],
    last_idx: i32,
    last_mask: u16,
) -> i32 {
    let any_match = bag.iter().any(|&idx| can_pick(pool, idx, last_idx, last_mask));

    let chosen = loop {
        let r = rng.next_double() * bag.len() as f64;
        let pick = bag[r as usize % bag.len()];
        if !any_match || can_pick(pool, pick, last_idx, last_mask) {
            break pick;
        }
    };

    // Remove first occurrence (List<T>.Remove behaviour)
    if let Some(pos) = bag.iter().position(|&x| x == chosen) {
        bag.remove(pos);
    }
    chosen
}

// ---------------------------------------------------------------------------
// SimulateEncounters
//
// Returns boss index. If weak_out is Some, records the first N weak picks.
// Critical: first regular pick is constrained by last weak pick (same list
// in the game — _rooms.normalEncounters contains both weak + regular).
// ---------------------------------------------------------------------------
pub fn simulate_encounters(
    rng: &mut Rng,
    act: &ActPools,
    mut weak_out: Option<&mut [i32]>,
) -> i32 {
    let mut weak_bag: Vec<i32> = Vec::with_capacity(act.weak.len());
    let mut last_idx: i32 = -1;
    let mut last_mask: u16 = 0;

    // Weak encounters
    for i in 0..act.num_weak {
        if weak_bag.is_empty() {
            weak_bag.extend(0..act.weak.len() as i32);
        }
        let idx = grab_bag_pick(rng, &mut weak_bag, act.weak, last_idx, last_mask);
        if let Some(ref mut out) = weak_out {
            if i < out.len() {
                out[i] = idx;
            }
        }
        last_idx = idx;
        last_mask = act.weak[idx as usize].tags;
    }

    // Regular encounters — inherit last weak pick as initial constraint
    let mut reg_bag: Vec<i32> = Vec::with_capacity(act.reg.len());
    for _ in act.num_weak..act.base_rooms {
        if reg_bag.is_empty() {
            reg_bag.extend(0..act.reg.len() as i32);
        }
        let idx = grab_bag_pick(rng, &mut reg_bag, act.reg, last_idx, last_mask);
        last_idx = idx;
        last_mask = act.reg[idx as usize].tags;
    }

    // Elite encounters (always 15 rooms)
    let mut elite_bag: Vec<i32> = Vec::with_capacity(act.elite.len());
    last_idx = -1;
    last_mask = 0;
    for _ in 0..15 {
        if elite_bag.is_empty() {
            elite_bag.extend(0..act.elite.len() as i32);
        }
        let idx = grab_bag_pick(rng, &mut elite_bag, act.elite, last_idx, last_mask);
        last_idx = idx;
        last_mask = act.elite[idx as usize].tags;
    }

    // Boss pick
    rng.next_item(act.boss.len() as i32)
}

// Convenience: simulate encounters, discard weak picks, just return boss index.
pub fn sim_enc(rng: &mut Rng, act: &ActPools) -> i32 {
    simulate_encounters(rng, act, None)
}

// Simulate encounters and record the first `n` weak picks into a fixed array.
pub fn sim_enc_weak<const N: usize>(rng: &mut Rng, act: &ActPools) -> ([i32; N], i32) {
    let mut out = [-1i32; N];
    let boss = simulate_encounters(rng, act, Some(&mut out));
    (out, boss)
}
