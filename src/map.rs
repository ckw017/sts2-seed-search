/// Port of STS2's StandardActMap generation.
///
/// Sources traced from sts2.decompiled.cs:
///   StandardActMap (line 362584) — GenerateMap, AssignPointTypes, pruning, post-processing
///   MapPathPruning (line 361137) — PruneDuplicateSegments
///   MapPostProcessing (line 361771) — CenterGrid, SpreadAdjacentMapPoints, StraightenPaths
///   MapPointTypeCounts (line 361746)
///   ActModel (Overgrowth line 359800) — GetMapPointTypes
///
/// Constructor order (line 362641–362663):
///   1. GetMapPointTypes  (Gaussian RNG calls)
///   2. GenerateMap       (path shuffles)
///   3. AssignPointTypes  (StableShuffle RNG)
///   4. PruneDuplicateSegments (UnstableShuffle RNG)
///   5. CenterGrid / SpreadAdjacentMapPoints / StraightenPaths  (no RNG)
///
/// For the row-2 Unknown constraint, we need steps 1–4 (post-processing only
/// moves nodes within rows, not between them).

use std::collections::{BTreeMap, BTreeSet};
use crate::rng::Rng;
use crate::seed::make_rng_seed;

// Grid is 7 columns wide. MAX_ROWS is the array allocation size (largest _mapLength).
const W: usize = 7;
const MAX_ROWS: usize = 16; // largest _mapLength (Overgrowth BaseNumberOfRooms=15+1)

// Ancient is always at col 3, row 0 (StartingMapPoint).
const ANCIENT_COL: usize = 3;
const ANCIENT_ROW: usize = 0;

// IsValidForLower threshold (C# line 362967): row < 5 → no RestSite or Elite.
const LOWER_ROW_LIMIT: usize = 5;

// Per-act map parameters — derived from _mapLength = BaseNumberOfRooms + 1.
struct MapCfg {
    rows:         usize, // _mapLength (grid rows = 0..rows-1; boss at virtual row=rows)
    rest_row:     usize, // rows - 1
    treasure_row: usize, // rows - 7
    upper_limit:  usize, // rows - 3  (row >= upper_limit → no RestSite)
}

const OVERGROWTH_CFG: MapCfg = MapCfg { rows: 16, rest_row: 15, treasure_row: 9,  upper_limit: 13 };
const HIVE_CFG:       MapCfg = MapCfg { rows: 15, rest_row: 14, treasure_row: 8,  upper_limit: 12 };

// MapPointType values (must match C# enum order used in segment keys).
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
#[repr(i32)]
pub enum PointType {
    Unassigned = 0,
    Monster    = 1,
    Elite      = 2,
    RestSite   = 3,
    Shop       = 4,
    Treasure   = 5,
    Unknown    = 6,
    Boss       = 7,
    Ancient    = 8,
}

// ---------------------------------------------------------------------------
// Node pool — indices are stable after allocation.
// ---------------------------------------------------------------------------

type Idx = usize;
const NULL: usize = usize::MAX;

struct Nodes {
    col:      Vec<i32>,
    row:      Vec<usize>,
    pt:       Vec<PointType>,
    modif:    Vec<bool>,
    parents:  Vec<BTreeSet<Idx>>,
    children: Vec<BTreeSet<Idx>>,
}

impl Nodes {
    fn new() -> Self {
        Self {
            col:      Vec::new(),
            row:      Vec::new(),
            pt:       Vec::new(),
            modif:    Vec::new(),
            parents:  Vec::new(),
            children: Vec::new(),
        }
    }
    fn alloc(&mut self, col: i32, row: usize) -> Idx {
        let id = self.col.len();
        self.col.push(col);
        self.row.push(row);
        self.pt.push(PointType::Unassigned);
        self.modif.push(true);
        self.parents.push(BTreeSet::new());
        self.children.push(BTreeSet::new());
        id
    }
    fn add_child(&mut self, parent: Idx, child: Idx) {
        self.children[parent].insert(child);
        self.parents[child].insert(parent);
    }
    fn remove_child(&mut self, parent: Idx, child: Idx) {
        self.children[parent].remove(&child);
        self.parents[child].remove(&parent);
    }
}

// ---------------------------------------------------------------------------
// Map state
// ---------------------------------------------------------------------------

struct ActMap {
    n:            Nodes,
    grid:         [[Option<Idx>; MAX_ROWS]; W], // grid[col][row], valid rows 0..cfg.rows-1
    boss:         Idx,       // at col 3, virtual row = cfg.rows
    ancient:      Idx,       // at col 3, row 0
    start_points: BTreeSet<Idx>,
    cfg:          &'static MapCfg,
}

impl ActMap {
    fn new(cfg: &'static MapCfg) -> Self {
        let mut n = Nodes::new();
        let boss    = n.alloc(ANCIENT_COL as i32, cfg.rows); // virtual, outside grid
        n.pt[boss]  = PointType::Boss;
        let ancient = n.alloc(ANCIENT_COL as i32, ANCIENT_ROW);
        n.pt[ancient] = PointType::Ancient;
        ActMap {
            n,
            grid: [[None; MAX_ROWS]; W],
            boss,
            ancient,
            start_points: BTreeSet::new(),
            cfg,
        }
    }

    fn get_or_create(&mut self, col: usize, row: usize) -> Idx {
        if let Some(id) = self.grid[col][row] { return id; }
        let id = self.n.alloc(col as i32, row);
        self.grid[col][row] = Some(id);
        id
    }

    /// C# Grid[targetX, current.row] — only the grid array, not ancient/boss.
    fn grid_get(&self, col: i32, row: usize) -> Option<Idx> {
        if col < 0 || col >= W as i32 { return None; }
        self.grid[col as usize][row]
    }

    /// HasInvalidCrossover (line 362738).
    fn has_invalid_crossover(&self, cur_col: i32, cur_row: usize, target_col: i32) -> bool {
        let delta = target_col - cur_col;
        if delta == 0 { return false; }
        // num == 7 case in C# is dead code; only delta==0 matters.
        if let Some(id) = self.grid_get(target_col, cur_row) {
            for &child in &self.n.children[id] {
                let child_delta = self.n.col[child] - target_col;
                if child_delta == -delta { return true; }
            }
        }
        false
    }

    /// GenerateNextCoord (line 362699): StableShuffle([-1,0,1]), pick first non-crossing.
    fn generate_next_coord(&self, rng: &mut Rng, col: i32, row: usize) -> (i32, usize) {
        let num  = (col - 1).max(0);     // clamped left
        let num2 = (col + 1).min(W as i32 - 1); // clamped right
        // StableShuffle of [-1,0,1] (already sorted) = UnstableShuffle.
        let mut offsets = [-1i32, 0, 1];
        // UnstableShuffle: num=3, 2 swaps.
        {
            let mut n = 3usize;
            while n > 1 {
                n -= 1;
                let j = rng.next_int(n as i32 + 1) as usize;
                offsets.swap(j, n);
            }
        }
        for &off in &offsets {
            let nc = match off {
                -1 => num,
                0  => col,
                1  => num2,
                _  => unreachable!(),
            };
            if !self.has_invalid_crossover(col, row, nc) {
                return (nc, row + 1);
            }
        }
        panic!("Cannot find next node at col={col} row={row}");
    }

    /// PathGenerate (line 362687): walk from startId (row 1) up through last row.
    fn path_generate(&mut self, rng: &mut Rng, start_id: Idx) {
        let mut cur_id = start_id;
        // while cur_row < _mapLength - 1
        let top = self.cfg.rows - 1;
        while self.n.row[cur_id] < top {
            let col = self.n.col[cur_id];
            let row = self.n.row[cur_id];
            let (nc, nr) = self.generate_next_coord(rng, col, row);
            let child_id = self.get_or_create(nc as usize, nr);
            self.n.add_child(cur_id, child_id);
            cur_id = child_id;
        }
    }

    /// GenerateMap (line 362761).
    fn generate_map(&mut self, rng: &mut Rng) {
        for i in 0..7usize {
            let mut col = rng.next_int(W as i32) as usize;
            let mut id  = self.get_or_create(col, 1);
            if i == 1 {
                while self.start_points.contains(&id) {
                    col = rng.next_int(W as i32) as usize;
                    id  = self.get_or_create(col, 1);
                }
            }
            self.start_points.insert(id);
            self.path_generate(rng, id);
        }
        // Connect last-row nodes → boss.
        let last = self.cfg.rows - 1;
        for col in 0..W {
            if let Some(id) = self.grid[col][last] {
                self.n.add_child(id, self.boss);
            }
        }
        // Connect ancient → all row-1 nodes.
        for col in 0..W {
            if let Some(id) = self.grid[col][1] {
                self.n.add_child(self.ancient, id);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// AssignPointTypes
// ---------------------------------------------------------------------------

/// IsValidForLower (row < 5 → no RestSite or Elite).
fn is_valid_lower(pt: PointType, row: usize) -> bool {
    if row < LOWER_ROW_LIMIT {
        return !matches!(pt, PointType::RestSite | PointType::Elite);
    }
    true
}

/// IsValidForUpper (row >= upper_limit → no RestSite).
fn is_valid_upper(pt: PointType, row: usize, upper_limit: usize) -> bool {
    if row >= upper_limit {
        return pt != PointType::RestSite;
    }
    true
}

/// Parent/child restriction (Elite/Rest/Treasure/Shop cannot be adjacent to same type).
fn is_valid_parents_children(nodes: &Nodes, pt: PointType, id: Idx) -> bool {
    if !matches!(pt, PointType::Elite | PointType::RestSite | PointType::Treasure | PointType::Shop) {
        return true;
    }
    for &p in &nodes.parents[id] {
        if nodes.pt[p] == pt { return false; }
    }
    for &c in &nodes.children[id] {
        if nodes.pt[c] == pt { return false; }
    }
    true
}

/// Sibling restriction (Rest/Monster/Unknown/Elite/Shop cannot have same-type sibling).
fn is_valid_siblings(nodes: &Nodes, pt: PointType, id: Idx) -> bool {
    if !matches!(pt, PointType::RestSite | PointType::Monster | PointType::Unknown
                   | PointType::Elite | PointType::Shop) {
        return true;
    }
    // Siblings = children of my parents that aren't me.
    for &p in &nodes.parents[id] {
        for &sib in &nodes.children[p] {
            if sib != id && nodes.pt[sib] == pt { return false; }
        }
    }
    true
}

fn is_valid(nodes: &Nodes, pt: PointType, id: Idx, upper_limit: usize) -> bool {
    let row = nodes.row[id];
    is_valid_lower(pt, row)
        && is_valid_upper(pt, row, upper_limit)
        && is_valid_parents_children(nodes, pt, id)
        && is_valid_siblings(nodes, pt, id)
}

/// GetNextValidPointType (line 362922): rotate queue, find first valid type.
fn get_next_valid(nodes: &Nodes, queue: &mut std::collections::VecDeque<PointType>, id: Idx, upper_limit: usize) -> PointType {
    let len = queue.len();
    for _ in 0..len {
        let pt = queue.pop_front().unwrap();
        if is_valid(nodes, pt, id, upper_limit) {
            return pt;
        }
        queue.push_back(pt);
    }
    PointType::Unassigned
}

fn assign_point_types(map: &mut ActMap, rng: &mut Rng, num_rests: i32, num_unknowns: i32) {
    let num_elites = 5;
    let num_shops  = 3;

    let rest_row     = map.cfg.rest_row;
    let treasure_row = map.cfg.treasure_row;
    let upper_limit  = map.cfg.upper_limit;

    // Fixed rows (C# lines 362804–362828).
    // Rest row → RestSite, not modifiable.
    for col in 0..W {
        if let Some(id) = map.grid[col][rest_row] {
            map.n.pt[id] = PointType::RestSite;
            map.n.modif[id] = false;
        }
    }
    // Treasure row → Treasure, not modifiable.
    for col in 0..W {
        if let Some(id) = map.grid[col][treasure_row] {
            map.n.pt[id] = PointType::Treasure;
            map.n.modif[id] = false;
        }
    }
    // Row 1 → Monster.
    for col in 0..W {
        if let Some(id) = map.grid[col][1] {
            map.n.pt[id] = PointType::Monster;
        }
    }

    // Build queue.
    let mut queue: std::collections::VecDeque<PointType> = std::collections::VecDeque::new();
    for _ in 0..num_rests   { queue.push_back(PointType::RestSite); }
    for _ in 0..num_shops   { queue.push_back(PointType::Shop); }
    for _ in 0..num_elites  { queue.push_back(PointType::Elite); }
    for _ in 0..num_unknowns { queue.push_back(PointType::Unknown); }

    // AssignRemainingTypesToRandomPoints: StableShuffle all nodes, assign unassigned.
    // Collect all grid nodes (boss and ancient are off-grid).
    let mut all_ids: Vec<Idx> = {
        let mut v = Vec::new();
        for c in 0..W {
            for r in 0..map.cfg.rows {
                if let Some(id) = map.grid[c][r] { v.push(id); }
            }
        }
        v
    };
    // StableShuffle: sort by MapPoint.CompareTo = MapCoord.CompareTo = (col, row).
    all_ids.sort_by_key(|&id| (map.n.col[id], map.n.row[id] as i32));
    // Then UnstableShuffle.
    {
        let len = all_ids.len();
        let mut n = len;
        while n > 1 {
            n -= 1;
            let j = rng.next_int(n as i32 + 1) as usize;
            all_ids.swap(j, n);
        }
    }
    for &id in &all_ids {
        if map.n.pt[id] != PointType::Unassigned { continue; }
        let pt = get_next_valid(&map.n, &mut queue, id, upper_limit);
        map.n.pt[id] = pt;
    }
    // Remaining Unassigned → Monster.
    let num_nodes = map.n.col.len();
    for id in 0..num_nodes {
        if map.n.pt[id] == PointType::Unassigned { map.n.pt[id] = PointType::Monster; }
    }
    // Boss and Ancient types (set at allocation, confirmed here).
    map.n.pt[map.boss]    = PointType::Boss;
    map.n.pt[map.ancient] = PointType::Ancient;
}

// ---------------------------------------------------------------------------
// PruneDuplicateSegments
// ---------------------------------------------------------------------------

/// FindAllPaths from startingMapPoint (DFS). C# line 361165.
/// Builds all root→boss paths (each path = list of Idx from start to boss).
fn find_all_paths(nodes: &Nodes, start: Idx) -> Vec<Vec<Idx>> {
    let mut result = Vec::new();
    if nodes.pt[start] == PointType::Boss {
        result.push(vec![start]);
        return result;
    }
    for &child in &nodes.children[start] {
        for mut sub_path in find_all_paths(nodes, child) {
            let mut path = vec![start];
            path.append(&mut sub_path);
            result.push(path);
        }
    }
    result
}

/// IsValidSegmentStartMapPoint (line 361232).
fn is_valid_seg_start(nodes: &Nodes, id: Idx) -> bool {
    if nodes.children[id].len() <= 1 {
        return nodes.row[id] == 0; // row==0 means ancient
    }
    true
}

/// IsValidSegmentEndMapPoint (line 361241).
fn is_valid_seg_end(nodes: &Nodes, id: Idx) -> bool {
    nodes.parents[id].len() >= 2
}

/// GenerateSegmentKey (line 361246).
fn segment_key(nodes: &Nodes, segment: &[Idx]) -> String {
    let start = segment[0];
    let end   = segment[segment.len() - 1];
    let types: Vec<i32> = segment.iter().map(|&id| nodes.pt[id] as i32).collect();
    let type_str = types.iter().map(|t| t.to_string()).collect::<Vec<_>>().join(",");
    if nodes.row[start] == 0 {
        // start is row 0 (ancient)
        format!("{}-{},{}-{}", nodes.row[start], nodes.col[end], nodes.row[end], type_str)
    } else {
        format!("{},{}-{},{}-{}", nodes.col[start], nodes.row[start],
                                  nodes.col[end],   nodes.row[end], type_str)
    }
}

/// AnyOverlappingSegments / OverlappingSegment (line 361283–361301).
fn overlapping(a: &[Idx], b: &[Idx]) -> bool {
    if a.len() < 3 || b.len() < 3 { return false; }
    for i in 1..=(a.len() - 2).min(b.len() - 2) {
        if a[i] == b[i] { return true; }
    }
    false
}

/// AddSegmentsToDictionary (line 361198).
fn add_segments(nodes: &Nodes, path: &[Idx], segments: &mut BTreeMap<String, Vec<Vec<Idx>>>) {
    for i in 0..path.len().saturating_sub(1) {
        if !is_valid_seg_start(nodes, path[i]) { continue; }
        for j in (i + 2)..path.len() {
            let end = path[j];
            if !is_valid_seg_end(nodes, end) { continue; }
            let seg: Vec<Idx> = path[i..=j].to_vec();
            let key = segment_key(nodes, &seg);
            let entry = segments.entry(key).or_insert_with(Vec::new);
            let any_overlap = entry.iter().any(|existing| overlapping(existing, &seg));
            if !any_overlap {
                entry.push(seg);
            }
        }
    }
}

/// FindMatchingSegments (line 361154): returns groups with >1 duplicate.
fn find_matching_segments(nodes: &Nodes, ancient: Idx) -> Vec<Vec<Vec<Idx>>> {
    let paths = find_all_paths(nodes, ancient);
    let mut segments: BTreeMap<String, Vec<Vec<Idx>>> = BTreeMap::new();
    for path in &paths {
        add_segments(nodes, path, &mut segments);
    }
    segments.into_values().filter(|v| v.len() > 1).collect()
}

/// IsInMap (line 361388): true if node is in grid, or is Ancient or Boss.
fn is_in_map(map: &ActMap, id: Idx) -> bool {
    if id == map.boss || id == map.ancient { return true; }
    let col = map.n.col[id] as usize;
    let row = map.n.row[id];
    map.grid[col][row] == Some(id)
}

/// IsRemoved (line 361397): true if grid slot is null.
/// Unlike IsInMap, Ancient returns true (Ancient not stored in grid).
/// Boss is never removed (explicit check, because Hive boss row=15 is within MAX_ROWS bounds).
fn is_removed(map: &ActMap, id: Idx) -> bool {
    if id == map.boss { return false; }
    let row = map.n.row[id];
    if row >= MAX_ROWS { return false; } // safety: out-of-bounds = not in grid
    let col = map.n.col[id] as usize;
    map.grid[col][row].is_none()
}

/// RemovePoint (line 361374): remove from grid, disconnect all edges.
fn remove_point(map: &mut ActMap, id: Idx) {
    let col = map.n.col[id] as usize;
    let row = map.n.row[id];
    map.grid[col][row] = None;
    map.start_points.remove(&id);
    let children: Vec<Idx> = map.n.children[id].iter().copied().collect();
    for c in children { map.n.remove_child(id, c); }
    let parents: Vec<Idx> = map.n.parents[id].iter().copied().collect();
    for p in parents { map.n.remove_child(p, id); }
}

/// PruneSegment (line 361343).
fn prune_segment(map: &mut ActMap, segment: &[Idx]) -> bool {
    let mut result = false;
    for i in 0..segment.len().saturating_sub(1) {
        let id = segment[i];
        if !is_in_map(map, id) { return true; }
        // Skip if branching, merging, or a parent would lose its only child.
        // C# uses !IsRemoved (not IsInMap) for the parent check.
        let skip = map.n.children[id].len() > 1
            || map.n.parents[id].len() > 1
            || map.n.parents[id].iter().any(|&p| {
                map.n.children[p].len() == 1 && !is_removed(map, p)
            });
        if skip { continue; }

        // Check remaining sub-segment for diverging nodes (>1 children, 1 parent).
        let has_diverging = segment[i..].iter().any(|&n| {
            map.n.children[n].len() > 1 && map.n.parents[n].len() == 1
        });
        if has_diverging { continue; }

        let last = segment[segment.len() - 1];
        if map.n.parents[last].len() == 1 { return false; }

        // Remove if no non-segment child would become parentless.
        let any_outside_would_orphan = map.n.children[id].iter()
            .filter(|&&c| !segment.contains(&c))
            .any(|&c| map.n.parents[c].len() == 1);
        if !any_outside_would_orphan {
            remove_point(map, id);
            result = true;
        }
    }
    result
}

/// PruneAllButLast (line 361326).
/// C# exits when `num == matches.Count - 1` (count-based, not index-based).
/// This means if segment[0] fails to prune, segment[1] is still tried.
fn prune_all_but_last(map: &mut ActMap, matches: &[Vec<Idx>]) -> usize {
    let mut count = 0;
    for seg in matches.iter() {
        if count == matches.len() - 1 { return count; }
        if prune_segment(map, seg) { count += 1; }
    }
    count
}

/// BreakAParentChildRelationshipInSegment (line 361414).
fn break_parent_child_in_segment(map: &mut ActMap, segment: &[Idx]) -> bool {
    let mut result = false;
    for i in 0..segment.len().saturating_sub(1) {
        let id = segment[i];
        if map.n.children[id].len() >= 2 {
            let next = segment[i + 1];
            if map.n.parents[next].len() != 1 {
                map.n.remove_child(id, next);
                result = true;
            }
        }
    }
    result
}

/// PrunePaths (line 361309): one round of pruning.
fn prune_paths(map: &mut ActMap, rng: &mut Rng, matching: &mut Vec<Vec<Vec<Idx>>>) -> bool {
    for group in matching.iter_mut() {
        // UnstableShuffle the group (list of duplicate segments).
        let mut n = group.len();
        while n > 1 {
            n -= 1;
            let j = rng.next_int(n as i32 + 1) as usize;
            group.swap(j, n);
        }
        if prune_all_but_last(map, group) != 0 { return true; }
        // Try breaking a parent-child relationship.
        for seg in group.iter() {
            if break_parent_child_in_segment(map, seg) { return true; }
        }
    }
    false
}

/// PruneDuplicateSegments (line 361139).
fn prune_duplicate_segments(map: &mut ActMap, rng: &mut Rng) {
    let mut matching = find_matching_segments(&map.n, map.ancient);
    let mut iters = 0;
    while prune_paths(map, rng, &mut matching) {
        iters += 1;
        assert!(iters <= 50, "Pruning exceeded 50 iterations");
        matching = find_matching_segments(&map.n, map.ancient);
    }
}

// ---------------------------------------------------------------------------
// Top-level: generate the full map and return the final grid.
// ---------------------------------------------------------------------------

/// Overgrowth GetMapPointTypes (line 359800):
///   NextGaussianInt(7,1,6,7) → num_rests
///   new MapPointTypeCounts(rng): Gaussian(12,1,10,14) → num_unknowns, Gaussian(5,1,3,6) → discard
fn overgrowth_map_point_types(rng: &mut Rng) -> (i32, i32) {
    let num_rests    = rng.next_gaussian_int(7, 1.0, 6, 7);
    let num_unknowns = rng.next_gaussian_int(12, 1.0, 10, 14);
    let _discard     = rng.next_gaussian_int(5, 1.0, 3, 6);
    (num_rests, num_unknowns)
}

/// Hive GetMapPointTypes (line 359674):
///   Forks the rng (same Seed+Counter) → fork.Gaussian(12,1,10,14) = copy_unknowns
///   Real rng: Gaussian(6,1,6,7) → num_rests, then MapPointTypeCounts ctor:
///     Gaussian(12,1,10,14) + Gaussian(5,1,3,6) (both discarded — overridden by fork values)
///   Result: num_unknowns = copy_unknowns - 1, num_rests from Gaussian(6,1,6,7).
fn hive_map_point_types(rng: &mut Rng, map_seed: u32) -> (i32, i32) {
    // Fork: same Seed and Counter as rng right now (Counter=0 for a fresh map rng).
    let copy_unknowns = {
        let mut copy = Rng::new(map_seed, rng.counter);
        copy.next_gaussian_int(12, 1.0, 10, 14)
    };
    let num_rests = rng.next_gaussian_int(6, 1.0, 6, 7);
    let _d1 = rng.next_gaussian_int(12, 1.0, 10, 14); // MapPointTypeCounts ctor on real rng (discarded)
    let _d2 = rng.next_gaussian_int(5, 1.0, 3, 6);    // ditto
    (num_rests, copy_unknowns - 1)
}

/// Full map generation: GetMapPointTypes → GenerateMap → AssignPointTypes → PruneDuplicateSegments.
/// Post-processing (CenterGrid etc.) omitted — only changes column positions, not rows.
fn generate_map_full(rng: &mut Rng, cfg: &'static MapCfg, map_seed: u32) -> (ActMap, i32) {
    let (num_rests, num_unknowns) = if std::ptr::eq(cfg, &HIVE_CFG) {
        hive_map_point_types(rng, map_seed)
    } else {
        overgrowth_map_point_types(rng)
    };

    let mut map = ActMap::new(cfg);
    map.generate_map(rng);
    assign_point_types(&mut map, rng, num_rests, num_unknowns);
    prune_duplicate_segments(&mut map, rng);

    (map, num_unknowns)
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Returns true if any node in row 2 of the Act 1 map is Unknown (event room).
pub fn act1_map_has_unknown_row2(run_seed: u32, _is_underdocks: bool) -> bool {
    let map_seed = make_rng_seed(run_seed, "act_1_map");
    let mut rng = Rng::new(map_seed, 0);
    let (map, _) = generate_map_full(&mut rng, &OVERGROWTH_CFG, map_seed);
    (0..W).any(|col| map.grid[col][2].map(|id| map.n.pt[id] == PointType::Unknown).unwrap_or(false))
}

/// Returns true if any node in row 2 of the Act 2 (Hive) map is Unknown (event room).
pub fn act2_map_has_unknown_row2(run_seed: u32) -> bool {
    let map_seed = make_rng_seed(run_seed, "act_2_map");
    let mut rng = Rng::new(map_seed, 0);
    let (map, _) = generate_map_full(&mut rng, &HIVE_CFG, map_seed);
    (0..W).any(|col| map.grid[col][2].map(|id| map.n.pt[id] == PointType::Unknown).unwrap_or(false))
}

fn print_map(map: &ActMap, label: &str, num_unknowns: i32) {
    eprintln!("{label} (num_unknowns={num_unknowns})");
    for row in 0..map.cfg.rows {
        let nodes: Vec<String> = (0..W).map(|col| {
            match map.grid[col][row] {
                None => "   ".to_string(),
                Some(id) => format!("{}{}", col, match map.n.pt[id] {
                    PointType::Monster    => "M",
                    PointType::Elite      => "E",
                    PointType::RestSite   => "R",
                    PointType::Shop       => "S",
                    PointType::Treasure   => "T",
                    PointType::Unknown    => "?",
                    PointType::Boss       => "B",
                    PointType::Ancient    => "A",
                    PointType::Unassigned => "_",
                }),
            }
        }).collect();
        eprintln!("Row {:2}: {}", row, nodes.join(" "));
    }
}

/// Debug: print the Act 1 map (pre-post-processing column positions).
pub fn print_act1_map(run_seed: u32, _is_underdocks: bool) {
    let map_seed = make_rng_seed(run_seed, "act_1_map");
    let mut rng = Rng::new(map_seed, 0);
    let (map, nu) = generate_map_full(&mut rng, &OVERGROWTH_CFG, map_seed);
    print_map(&map, "Act1 map", nu);
}

/// Debug: print the Act 2 map (pre-post-processing column positions).
pub fn print_act2_map(run_seed: u32) {
    let map_seed = make_rng_seed(run_seed, "act_2_map");
    let mut rng = Rng::new(map_seed, 0);
    let (map, nu) = generate_map_full(&mut rng, &HIVE_CFG, map_seed);
    print_map(&map, "Act2 map", nu);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed::get_deterministic_hash_code;

    fn run_seed(s: &str) -> u32 {
        get_deterministic_hash_code(s) as u32
    }

    #[test]
    fn test_trace_14dkk9mh_start_cols() {
        let rs = run_seed("14DKK9MH");
        let map_seed = make_rng_seed(rs, "act_1_map");
        let mut rng = Rng::new(map_seed, 0);
        // Gaussian calls (consume raw doubles, no counter)
        let num_rests    = rng.next_gaussian_int(7, 1.0, 6, 7);
        let num_unknowns = rng.next_gaussian_int(12, 1.0, 10, 14);
        let _discard     = rng.next_gaussian_int(5, 1.0, 3, 6);
        eprintln!("After Gaussians: counter={}, num_rests={num_rests}, num_unknowns={num_unknowns}", rng.counter);
        // First 7 NextInt calls for start columns
        let cols: Vec<i32> = (0..7).map(|_| rng.next_int(7)).collect();
        eprintln!("Start cols (first 7 draws): {:?}", cols);
    }

    #[test]
    fn test_trace_14dkk9mh_pruning() {
        let rs = run_seed("14DKK9MH");
        let map_seed = make_rng_seed(rs, "act_1_map");
        let mut rng = Rng::new(map_seed, 0);
        let num_rests    = rng.next_gaussian_int(7, 1.0, 6, 7);
        let num_unknowns = rng.next_gaussian_int(12, 1.0, 10, 14);
        let _discard     = rng.next_gaussian_int(5, 1.0, 3, 6);
        let mut map = ActMap::new(&OVERGROWTH_CFG);
        map.generate_map(&mut rng);
        assign_point_types(&mut map, &mut rng, num_rests, num_unknowns);

        // Print the map before pruning
        eprintln!("=== PRE-PRUNE MAP ===");
        for row in 1..5 {
            let nodes: Vec<String> = (0..W).map(|col| {
                match map.grid[col][row] {
                    None => "  ".to_string(),
                    Some(id) => format!("{}{}", col, match map.n.pt[id] {
                        PointType::Monster => "M", PointType::Elite => "E",
                        PointType::RestSite => "R", PointType::Shop => "S",
                        PointType::Treasure => "T", PointType::Unknown => "?",
                        _ => "_",
                    }),
                }
            }).collect();
            eprintln!("Row {:2}: {}", row, nodes.join(" "));
        }

        // Find matching segments before pruning
        let matching = find_matching_segments(&map.n, map.ancient);
        eprintln!("=== MATCHING SEGMENTS ({} groups) ===", matching.len());
        for (gi, group) in matching.iter().enumerate() {
            eprintln!("Group {gi} ({} segments):", group.len());
            for seg in group {
                let path_str: Vec<String> = seg.iter().map(|&id| {
                    format!("({},{}:{:?})", map.n.col[id], map.n.row[id], map.n.pt[id])
                }).collect();
                eprintln!("  [{}]", path_str.join(" → "));
            }
        }

        // Now prune and see what happens
        prune_duplicate_segments(&mut map, &mut rng);
        let row1: Vec<usize> = (0..W).filter(|&c| map.grid[c][1].is_some()).collect();
        eprintln!("Row 1 after prune: {:?}", row1);
    }

    #[test]
    fn test_trace_14dkk9mh_pre_prune() {
        let rs = run_seed("14DKK9MH");
        let map_seed = make_rng_seed(rs, "act_1_map");
        let mut rng = Rng::new(map_seed, 0);
        // Gaussian calls
        let num_rests    = rng.next_gaussian_int(7, 1.0, 6, 7);
        let num_unknowns = rng.next_gaussian_int(12, 1.0, 10, 14);
        let _discard     = rng.next_gaussian_int(5, 1.0, 3, 6);
        // GenerateMap — step by step
        let mut map = ActMap::new(&OVERGROWTH_CFG);
        for i in 0..7usize {
            let mut col = rng.next_int(W as i32) as usize;
            let mut id  = map.get_or_create(col, 1);
            if i == 1 {
                while map.start_points.contains(&id) {
                    col = rng.next_int(W as i32) as usize;
                    id  = map.get_or_create(col, 1);
                }
            }
            map.start_points.insert(id);
            let before_path = rng.counter;
            map.path_generate(&mut rng, id);
            eprintln!("Path {i}: start_col={col}, id={id}, rng_calls_in_path={}", rng.counter - before_path);
            let row1: Vec<usize> = (0..W).filter(|&c| map.grid[c][1].is_some()).collect();
            eprintln!("  Row 1 cols so far: {:?}", row1);
        }
        // Connect last-row → boss, ancient → row-1
        let last = map.cfg.rows - 1;
        for col in 0..W {
            if let Some(id) = map.grid[col][last] { map.n.add_child(id, map.boss); }
        }
        for col in 0..W {
            if let Some(id) = map.grid[col][1] { map.n.add_child(map.ancient, id); }
        }
        let row1_final: Vec<usize> = (0..W).filter(|&c| map.grid[c][1].is_some()).collect();
        eprintln!("Row 1 cols after generate_map: {:?}", row1_final);
    }

    #[test]
    fn test_print_14dkk9mh() {
        print_act1_map(run_seed("14DKK9MH"), false);
    }

    /// Row 1 should have exactly 3 Monster nodes (confirmed by user in-game).
    #[test]
    fn test_14dkk9mh_row1() {
        let rs = run_seed("14DKK9MH");
        let map_seed = make_rng_seed(rs, "act_1_map");
        let mut rng = Rng::new(map_seed, 0);
        let (map, _) = generate_map_full(&mut rng, &OVERGROWTH_CFG, map_seed);
        let row1: Vec<(usize, PointType)> = (0..W)
            .filter_map(|c| map.grid[c][1].map(|id| (c, map.n.pt[id])))
            .collect();
        eprintln!("Row 1: {:?}", row1);
        assert_eq!(row1.len(), 3, "Row 1 should have 3 nodes");
        for (col, pt) in &row1 {
            assert_eq!(*pt, PointType::Monster, "Row 1 col {col} should be Monster");
        }
    }

    /// Print the Act 2 map for 14DKK9MH for manual verification.
    #[test]
    fn test_print_14dkk9mh_act2() {
        print_act2_map(run_seed("14DKK9MH"));
    }

    #[test]
    fn test_print_elm102x_act2() {
        print_act2_map(run_seed("ELM102X"));
    }

    #[test]
    fn test_fun_txt_act2_row2() {
        let seeds = [
            "9YZCBTR","9YZCET5","ELL0QSX","ELL1Q2X","ELM00SX","ELM102X",
            "PA396AQ","PA69SAQ","WBJ8ERK","WBJ9E3K","WCJXE3K","WCJYERK",
            "X0CYXYB","10PSENQS","10PSFN2S","13P0ENQS","13P0FN2S","14DJKXMH",
            "14DKK9MH","19SWAG58","1FHP1GFH","1GH11GFH","1K8B5RS7","1K8B6R07",
            "1K8B5WSR","1K8B6W0R","1K8G57S7","1K8G6707","1K8G52SR","1K8G620R",
            "1K9BTRS7","1K9BWR07","1K9BTWSR","1K9BWW0R","1K9GT7S7","1K9GW707",
            "1K9GT2SR","1K9GW20R","1NFJX58K","1TDFJ5UJ","203SENQS","203SFN2S",
            "2330ENQS","2330FN2S","248J4XMH","248K49MH",
        ];
        eprintln!("Seeds passing Act2 row2 event check:");
        for s in &seeds {
            let rs = run_seed(s);
            if act2_map_has_unknown_row2(rs) {
                eprintln!("  PASS: {s}");
            }
        }
        eprintln!("Seeds FAILING Act2 row2 event check:");
        for s in &seeds {
            let rs = run_seed(s);
            if !act2_map_has_unknown_row2(rs) {
                eprintln!("  FAIL: {s}");
            }
        }
    }
}
