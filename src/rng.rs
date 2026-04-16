/// Port of .NET's legacy System.Random (new Random(int seed)) algorithm.
///
/// .NET uses a subtractive Lagged Fibonacci generator from Knuth TAOCP.
/// Source: dotnet/runtime src/libraries/System.Private.CoreLib/src/System/Random.Legacy.cs
///
/// Verified to match C# output: new Random(12345).Next(100) × 10
///   → 6, 7, 77, 51, 79, 82, 16, 73, 26, 50

const MBIG: i32 = i32::MAX; // 2147483647
const MSEED: i32 = 161803398;

struct DotnetRandom {
    inext: usize,
    inextp: usize,
    seed_array: [i32; 56],
}

impl DotnetRandom {
    fn new(seed: i32) -> Self {
        let mut seed_array = [0i32; 56];
        let mut mj: i32;
        let mut mk: i32;

        let subtraction = if seed == i32::MIN {
            i32::MAX
        } else {
            seed.abs()
        };

        mj = MSEED.wrapping_sub(subtraction);
        seed_array[55] = mj;
        mk = 1;

        let mut ii: usize = 0;
        for i in 1usize..55 {
            ii += 21;
            if ii >= 55 {
                ii -= 55;
            }
            seed_array[ii] = mk;
            mk = mj.wrapping_sub(mk);
            if mk < 0 {
                mk = mk.wrapping_add(MBIG);
            }
            mj = seed_array[ii];
        }

        for _ in 1..=4 {
            for i in 1usize..=55 {
                let n = i + 30;
                let n = if n >= 55 { n - 55 } else { n };
                let v = seed_array[i].wrapping_sub(seed_array[1 + n]);
                seed_array[i] = if v < 0 { v.wrapping_add(MBIG) } else { v };
            }
        }

        DotnetRandom {
            inext: 0,
            inextp: 21,
            seed_array,
        }
    }

    fn internal_sample(&mut self) -> i32 {
        let mut inext = self.inext + 1;
        let mut inextp = self.inextp + 1;
        if inext >= 56 {
            inext = 1;
        }
        if inextp >= 56 {
            inextp = 1;
        }
        let mut ret_val = self.seed_array[inext].wrapping_sub(self.seed_array[inextp]);
        if ret_val == MBIG {
            ret_val -= 1;
        }
        if ret_val < 0 {
            ret_val = ret_val.wrapping_add(MBIG);
        }
        self.seed_array[inext] = ret_val;
        self.inext = inext;
        self.inextp = inextp;
        ret_val
    }

    /// Equivalent to C# Random.Next(max_value) — returns [0, max_value)
    fn next(&mut self, max_value: i32) -> i32 {
        assert!(max_value >= 0);
        if max_value == 0 {
            return 0;
        }
        let sample = self.internal_sample() as f64 * (1.0 / MBIG as f64);
        (sample * max_value as f64) as i32
    }

    /// Equivalent to C# Random.Next(min_value, max_value) — returns [min, max)
    fn next_range(&mut self, min_value: i32, max_value: i32) -> i32 {
        let range = (max_value - min_value) as i64;
        if range <= i32::MAX as i64 {
            self.next(range as i32) + min_value
        } else {
            // Large range — use double path (rare in this codebase)
            let sample = self.internal_sample() as f64 * (1.0 / MBIG as f64);
            (sample * range as f64 + min_value as f64) as i32
        }
    }

    /// Equivalent to C# Random.NextDouble()
    fn next_double(&mut self) -> f64 {
        self.internal_sample() as f64 * (1.0 / MBIG as f64)
    }
}

// ---------------------------------------------------------------------------
// Public Rng — mirrors the C# Rng class exactly
// ---------------------------------------------------------------------------

pub struct Rng {
    inner: DotnetRandom,
    pub counter: i32,
    pub seed: u32,
}

impl Rng {
    pub fn new(seed: u32, start_counter: i32) -> Self {
        let mut r = DotnetRandom::new(seed as i32);
        for _ in 0..start_counter {
            r.internal_sample();
        }
        Rng { inner: r, counter: start_counter, seed }
    }

    pub fn next_bool(&mut self) -> bool {
        self.counter += 1;
        self.inner.next(2) == 0
    }

    pub fn next_int(&mut self, max: i32) -> i32 {
        self.counter += 1;
        self.inner.next(max)
    }

    pub fn next_int_range(&mut self, min: i32, max: i32) -> i32 {
        self.counter += 1;
        self.inner.next_range(min, max)
    }

    pub fn next_double(&mut self) -> f64 {
        self.counter += 1;
        self.inner.next_double()
    }

    pub fn next_float(&mut self) -> f32 {
        self.counter += 1;
        self.inner.next_double() as f32
    }

    /// Equivalent to C# NextItem(count): returns index in [0, count) or -1 if count==0.
    pub fn next_item(&mut self, count: i32) -> i32 {
        self.counter += 1;
        if count == 0 {
            return -1;
        }
        self.inner.next_range(0, count)
    }

    /// Equivalent to C# NextGaussianInt(mean, stdDev, min, max).
    /// Uses Box-Muller; calls _random.NextDouble() directly (no Counter increment per call).
    /// Loops until result is in [min, max].
    /// Expose the raw internal_sample value without consuming Counter, for debugging.
    pub fn debug_internal_sample(&mut self) -> i32 {
        self.inner.internal_sample()
    }

    pub fn next_gaussian_int(&mut self, mean: i32, std_dev: f64, min: i32, max: i32) -> i32 {
        loop {
            let d = 1.0 - self.inner.next_double();
            let n = 1.0 - self.inner.next_double();
            let z = (-2.0 * d.ln()).sqrt() * (std::f64::consts::TAU * n).sin();
            let a = mean as f64 + std_dev * z;
            let result = a.round() as i32;
            if result >= min && result <= max {
                return result;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify against C#: new Random(12345).Next(100) × 10 → 6,7,77,51,79,82,16,73,26,50
    #[test]
    fn test_dotnet_random_matches_csharp() {
        let mut r = DotnetRandom::new(12345);
        let expected = [6, 7, 77, 51, 79, 82, 16, 73, 26, 50];
        for (i, &exp) in expected.iter().enumerate() {
            let got = r.next(100);
            assert_eq!(got, exp, "mismatch at index {i}: got {got}, expected {exp}");
        }
    }

    /// Verify Rng wrapper counter increments correctly
    #[test]
    fn test_rng_counter() {
        let mut rng = Rng::new(12345, 0);
        assert_eq!(rng.counter, 0);
        let v = rng.next_item(100);
        assert_eq!(rng.counter, 1);
        assert_eq!(v, 6);
    }

    /// Verify fast-forward: Rng(seed, N) should match calling Next() N times from scratch
    #[test]
    fn test_fast_forward() {
        let mut base = DotnetRandom::new(99999);
        for _ in 0..50 {
            base.internal_sample();
        }
        let expected = base.next(1000);

        let mut rng = Rng::new(99999, 50);
        let got = rng.next_int(1000);
        assert_eq!(got, expected, "fast-forward mismatch");
    }
}
