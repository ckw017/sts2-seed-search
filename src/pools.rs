// Encounter tags as bitmasks (ET enum in C#)
pub const TAG_WORKERS:      u16 = 1 << 0;
pub const TAG_CHOMPER:      u16 = 1 << 1;
pub const TAG_SLUGS:        u16 = 1 << 2;
pub const TAG_NIBBIT:       u16 = 1 << 3;
pub const TAG_SHRINKER:     u16 = 1 << 4;
pub const TAG_CRAWLER:      u16 = 1 << 5;
pub const TAG_MUSHROOM:     u16 = 1 << 6;
pub const TAG_SLIMES:       u16 = 1 << 7;
pub const TAG_EXOSKELETONS: u16 = 1 << 8;
pub const TAG_THIEVES:      u16 = 1 << 9;
pub const TAG_BURROWER:     u16 = 1 << 10;
pub const TAG_SCROLLS:      u16 = 1 << 11;
pub const TAG_SEAPUNK:      u16 = 1 << 12;
pub const TAG_KNIGHTS:      u16 = 1 << 13;

#[derive(Copy, Clone, Debug)]
pub struct Enc {
    pub tags: u16,
}

/// Encounter pool for one act + metadata
pub struct ActPools {
    pub weak:       &'static [Enc],
    pub reg:        &'static [Enc],
    pub elite:      &'static [Enc],
    pub boss:       &'static [Enc],
    pub num_weak:   usize,
    pub base_rooms: usize,
    pub events:     i32,
}

// ---------------------------------------------------------------------------
// Underdocks (Act1 variant)
// ---------------------------------------------------------------------------
static UDK_WEAK: [Enc; 4] = [
    Enc { tags: TAG_SLUGS },    // CorpseSlugsWeak
    Enc { tags: TAG_SEAPUNK },  // SeapunkWeak
    Enc { tags: TAG_WORKERS },  // SludgeSpinnerWeak
    Enc { tags: 0 },            // ToadpolesWeak
];
static UDK_REG: [Enc; 10] = [
    Enc { tags: TAG_SLUGS }, Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 },
    Enc { tags: 0 },         Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 },
];
static UDK_ELITE: [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];
static UDK_BOSS:  [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];

pub static UDK: ActPools = ActPools {
    weak: &UDK_WEAK, reg: &UDK_REG, elite: &UDK_ELITE, boss: &UDK_BOSS,
    num_weak: 3, base_rooms: 15, events: 10 + 18,
};

// Index constants for event queue tracking (all epochs revealed)
pub const DROWNING_BEACON_UDK_IDX: i32 = 1;   // AllEvents[1] in Underdocks list

// ---------------------------------------------------------------------------
// Overgrowth (Act1)
// ---------------------------------------------------------------------------
static OVG_WEAK: [Enc; 4] = [
    Enc { tags: TAG_CRAWLER },   // FuzzyWurmCrawlerWeak
    Enc { tags: TAG_NIBBIT },    // NibbitsWeak
    Enc { tags: TAG_SHRINKER },  // ShrinkerBeetleWeak
    Enc { tags: TAG_SLIMES },    // SlimesWeak
];
static OVG_REG: [Enc; 11] = [
    Enc { tags: 0 },                             // CubexConstruct
    Enc { tags: TAG_MUSHROOM | TAG_SLIMES },      // Flyconid
    Enc { tags: 0 },                             // Fogmog
    Enc { tags: 0 },                             // Inklets
    Enc { tags: 0 },                             // Mawler
    Enc { tags: 0 },                             // Nibbits
    Enc { tags: TAG_SHRINKER | TAG_CRAWLER },     // OvergrowthCrawlers
    Enc { tags: 0 },                             // RubyRaiders
    Enc { tags: TAG_MUSHROOM },                   // SnappingJaxfruit
    Enc { tags: 0 },                             // SlitheringStrangler
    Enc { tags: 0 },                             // VineShambler
];
static OVG_ELITE: [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];
static OVG_BOSS:  [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];

pub static OVG: ActPools = ActPools {
    weak: &OVG_WEAK, reg: &OVG_REG, elite: &OVG_ELITE, boss: &OVG_BOSS,
    num_weak: 3, base_rooms: 15, events: 13 + 18,
};

pub const DOLL_ROOM_OVG_IDX: i32 = 15; // Shared[2] in 31-event Ovg list

// ---------------------------------------------------------------------------
// Hive (Act2)
// ---------------------------------------------------------------------------
static HIVE_WEAK: [Enc; 4] = [
    Enc { tags: TAG_WORKERS },      // BowlbugsWeak       [0]
    Enc { tags: TAG_EXOSKELETONS }, // ExoskeletonsWeak   [1]
    Enc { tags: TAG_THIEVES },      // ThievingHopperWeak [2] ← ThievingHopperWeakIdx
    Enc { tags: TAG_BURROWER },     // TunnelerWeak       [3]
];
static HIVE_REG: [Enc; 11] = [
    Enc { tags: TAG_WORKERS },                   // BowlbugsNormal
    Enc { tags: TAG_CHOMPER },                   // ChompersNormal
    Enc { tags: TAG_EXOSKELETONS },              // ExoskeletonsNormal
    Enc { tags: 0 },                             // HunterKiller
    Enc { tags: 0 },                             // LouseProgenitor
    Enc { tags: 0 },                             // Mytes
    Enc { tags: 0 },                             // Ovicopter
    Enc { tags: TAG_WORKERS },                   // SlumberingBeetle
    Enc { tags: 0 },                             // SpinyToad
    Enc { tags: 0 },                             // TheObscura
    Enc { tags: TAG_BURROWER | TAG_WORKERS },    // TunnelerNormal
];
static HIVE_ELITE: [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];
static HIVE_BOSS:  [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];

pub static HIVE: ActPools = ActPools {
    weak: &HIVE_WEAK, reg: &HIVE_REG, elite: &HIVE_ELITE, boss: &HIVE_BOSS,
    num_weak: 2, base_rooms: 14, events: 10 + 18,
};

pub const DOLL_ROOM_HIVE_IDX:      i32 = 12; // Shared[2] in 28-event Hive list
pub const THIEVES_HOPPER_WEAK_IDX: i32 = 2;  // index 2 in HIVE_WEAK

// ---------------------------------------------------------------------------
// Glory (Act3)
// ---------------------------------------------------------------------------
static GLORY_WEAK: [Enc; 3] = [
    Enc { tags: 0 },             // DevotedSculptorWeak
    Enc { tags: TAG_SCROLLS },   // ScrollsOfBitingWeak
    Enc { tags: 0 },             // TurretOperatorWeak
];
static GLORY_REG: [Enc; 9] = [
    Enc { tags: 0 },            // Axebots
    Enc { tags: 0 },            // ConstructMenagerie
    Enc { tags: 0 },            // Fabricator
    Enc { tags: 0 },            // FrogKnight
    Enc { tags: 0 },            // GlobeHead
    Enc { tags: 0 },            // OwlMagistrate
    Enc { tags: TAG_SCROLLS },  // ScrollsOfBitingNormal
    Enc { tags: 0 },            // SlimedBerserker
    Enc { tags: 0 },            // TheLostAndForgotten
];
static GLORY_ELITE: [Enc; 3] = [
    Enc { tags: TAG_KNIGHTS }, // KnightsElite
    Enc { tags: 0 },           // MechaKnight
    Enc { tags: 0 },           // SoulNexus
];
static GLORY_BOSS: [Enc; 3] = [Enc { tags: 0 }, Enc { tags: 0 }, Enc { tags: 0 }];

pub static GLORY: ActPools = ActPools {
    weak: &GLORY_WEAK, reg: &GLORY_REG, elite: &GLORY_ELITE, boss: &GLORY_BOSS,
    num_weak: 2, base_rooms: 13, events: 7 + 18,
};

pub const REFLECTIONS_GLORY_IDX: i32 = 3; // Glory[3] in 25-event Glory list
pub const QUEEN_BOSS_IDX:         i32 = 1; // DoormakerBoss=0, QueenBoss=1, TestSubjectBoss=2

// Shop deque constants
pub const SHOP_DEQUE_SIZE:   i32 = 26; // 25 shared shop relics + UndyingSigil
pub const DOLLYS_MIRROR_IDX: i32 = 6;  // position in initial shop deque array
