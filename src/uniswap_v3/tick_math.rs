use alloy::primitives::{U256, I256}; 
use super::constants::*; 

/// @dev The minimum tick that may be passed to #getSqrtRatioAtTick computed from log base 1.0001 of 2**-128
pub const MIN_TICK: i32 = -887272;
/// @dev The maximum tick that may be passed to #getSqrtRatioAtTick computed from log base 1.0001 of 2**128 
pub const MAX_TICK: i32 = -MIN_TICK;

/// @dev The minimum value that can be returned from #getSqrtRatioAtTick. Equivalent to getSqrtRatioAtTick(MIN_TICK)
pub const MIN_SQRT_RATIO: U256 = U256::from_limbs([4295128739, 0, 0, 0]);
/// @dev The maximum value that can be returned from #getSqrtRatioAtTick. Equivalent to getSqrtRatioAtTick(MAX_TICK)
pub const MAX_SQRT_RATIO: U256 = U256::from_limbs([6743328256752651558, 17280870778742802505, 4294805859, 0]); 

pub const SQRT_10001: I256 = I256::from_raw(U256::from_limbs([11745905768312294533, 13863, 0, 0]));
pub const TICK_LOW: I256 = I256::from_raw(U256::from_limbs([
    6552757943157144234,
    184476617836266586,
    0,
    0,
]));

pub const TICK_HIGH: I256 = I256::from_raw(U256::from_limbs([
    4998474450511881007,
    15793544031827761793,
    0,
    0,
]));

/// @notice Calculates sqrt(1.0001^tick) * 2^96
/// @dev Throws if |tick| > max tick
/// @param tick The input tick for the above formula
/// @return sqrtPriceX96 A Fixed point Q64.96 number representing the sqrt of the ratio of the two assets (token1/token0)
/// at the given tick
pub fn get_sqrt_ratio_at_tick(tick: i32) -> Result<U256, String> {
    let abs_tick:U256 = U256::from(tick.unsigned_abs()); 

    if abs_tick > U256_MAX_TICK {
        return Err("Tick out of bounds".to_string());
    }

    let mut ratio = if abs_tick & (U256_1) != U256::ZERO {
        U256::from_limbs([12262481743371124737, 18445821805675392311, 0, 0])
    } else {
        U256::from_limbs([0, 0, 1, 0])
    };

    if !(abs_tick & U256_2).is_zero() {
        ratio = (ratio * U256::from_limbs([6459403834229662010, 18444899583751176498, 0, 0])) >> 128
    }
    if !(abs_tick & U256_4).is_zero() {
        ratio =
            (ratio * U256::from_limbs([17226890335427755468, 18443055278223354162, 0, 0])) >> 128
    }
    if !(abs_tick & U256_8).is_zero() {
        ratio = (ratio * U256::from_limbs([2032852871939366096, 18439367220385604838, 0, 0])) >> 128
    }
    if !(abs_tick & U256_16).is_zero() {
        ratio =
            (ratio * U256::from_limbs([14545316742740207172, 18431993317065449817, 0, 0])) >> 128
    }
    if !(abs_tick & U256_32).is_zero() {
        ratio = (ratio * U256::from_limbs([5129152022828963008, 18417254355718160513, 0, 0])) >> 128
    }
    if !(abs_tick & U256_64).is_zero() {
        ratio = (ratio * U256::from_limbs([4894419605888772193, 18387811781193591352, 0, 0])) >> 128
    }
    if !(abs_tick & U256_128).is_zero() {
        ratio = (ratio * U256::from_limbs([1280255884321894483, 18329067761203520168, 0, 0])) >> 128
    }
    if !(abs_tick & U256_256).is_zero() {
        ratio =
            (ratio * U256::from_limbs([15924666964335305636, 18212142134806087854, 0, 0])) >> 128
    }
    if !(abs_tick & U256_512).is_zero() {
        ratio = (ratio * U256::from_limbs([8010504389359918676, 17980523815641551639, 0, 0])) >> 128
    }
    if !(abs_tick & U256_1024).is_zero() {
        ratio =
            (ratio * U256::from_limbs([10668036004952895731, 17526086738831147013, 0, 0])) >> 128
    }
    if !(abs_tick & U256_2048).is_zero() {
        ratio = (ratio * U256::from_limbs([4878133418470705625, 16651378430235024244, 0, 0])) >> 128
    }
    if !(abs_tick & U256_4096).is_zero() {
        ratio = (ratio * U256::from_limbs([9537173718739605541, 15030750278693429944, 0, 0])) >> 128
    }
    if !(abs_tick & U256_8192).is_zero() {
        ratio = (ratio * U256::from_limbs([9972618978014552549, 12247334978882834399, 0, 0])) >> 128
    }
    if !(abs_tick & U256_16384).is_zero() {
        ratio = (ratio * U256::from_limbs([10428997489610666743, 8131365268884726200, 0, 0])) >> 128
    }
    if !(abs_tick & U256_32768).is_zero() {
        ratio = (ratio * U256::from_limbs([9305304367709015974, 3584323654723342297, 0, 0])) >> 128
    }
    if !(abs_tick & U256_65536).is_zero() {
        ratio = (ratio * U256::from_limbs([14301143598189091785, 696457651847595233, 0, 0])) >> 128
    }
    if !(abs_tick & U256_131072).is_zero() {
        ratio = (ratio * U256::from_limbs([7393154844743099908, 26294789957452057, 0, 0])) >> 128
    }
    if !(abs_tick & U256_262144).is_zero() {
        ratio = (ratio * U256::from_limbs([2209338891292245656, 37481735321082, 0, 0])) >> 128
    }
    if !(abs_tick & U256_524288).is_zero() {
        ratio = (ratio * U256::from_limbs([10518117631919034274, 76158723, 0, 0])) >> 128
    }

    if tick > 0 {
        ratio = U256::MAX / ratio;
    }

    Ok((ratio >> 32)
        + if (ratio.wrapping_rem(U256_1 << 32)).is_zero() {
            U256::ZERO
        } else {
            U256_1
        })
}

pub fn get_tick_at_sqrt_ratio(sqrt_price_x_96: U256) -> Result<i32, String> {
    if !(sqrt_price_x_96 >= MIN_SQRT_RATIO && sqrt_price_x_96 < MAX_SQRT_RATIO) {
        return Err("The price is out of bounds".to_string());
    }

    let ratio: U256 = sqrt_price_x_96<<32;
    let mut r = ratio;
    let mut msb = U256::ZERO;

    let mut f = if r > U256::from_limbs([18446744073709551615, 18446744073709551615, 0, 0]) {
        U256_1 << U256_7
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256::from_limbs([18446744073709551615, 0, 0, 0]) {
        U256_1 << U256_6
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256::from_limbs([4294967295, 0, 0, 0]) {
        U256_1 << U256_5
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256::from_limbs([65535, 0, 0, 0]) {
        U256_1 << U256_4
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256_255 {
        U256_1 << U256_3
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256_15 {
        U256_1 << U256_2
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256_3 {
        U256_1 << U256_1
    } else {
        U256::ZERO
    };
    msb = msb | f;
    r = r >> f;

    f = if r > U256_1 { U256_1 } else { U256::ZERO };

    msb = msb | f;

    r = if msb >= U256_128 {
        ratio >> (msb - U256_127)
    } else {
        ratio << (U256_127 - msb)
    };

    let mut log_2: I256 = (I256::from_raw(msb) - I256::from_limbs([128, 0, 0, 0]))<<64;

    for i in (51..=63).rev() {
        r = r.overflowing_mul(r).0>>U256_127;
        let f: U256 = r >> 128;
        log_2 = log_2 | I256::from_raw(f << i);

        r = r >> f;
    }

    r = r.overflowing_mul(r).0 >> U256_127;
    let f: U256 = r >> 128 ;
    log_2 = log_2|I256::from_raw(f << 50);

    let log_sqrt10001 = log_2.wrapping_mul(SQRT_10001);

    let tick_low = ((log_sqrt10001 - TICK_LOW) >> 128_u8).low_i32();

    let tick_high = ((log_sqrt10001 + TICK_HIGH) >> 128_u8).low_i32();

    let tick = if tick_low == tick_high {
        tick_low
    } else if get_sqrt_ratio_at_tick(tick_high)? <= sqrt_price_x_96 {
        tick_high
    } else {
        tick_low
    };

    Ok(tick)
}