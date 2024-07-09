use std::collections::HashMap;
use super::{liquidity_math::add_delta, tick_math::*}; 
use eyre::{eyre, Result}; 

#[derive(Default)]
pub struct Info {
    liquidity_gross: u128, 
    liquidity_net: i128, 
    initialized: bool
}

/// @notice Derives max liquidity per tick from given tick spacing
/// @dev Executed within the pool constructor
/// @param tickSpacing The amount of required tick separation, realized in multiples of `tickSpacing`
///     e.g., a tickSpacing of 3 requires ticks to be initialized every 3rd tick i.e., ..., -6, -3, 0, 3, 6, ...
/// @return The max liquidity per tick
pub fn _tick_spacing_to_max_liquidity_per_tick ( tick_spacing: i32 ) -> u128 {
    let min_tick: i32 = (MIN_TICK / tick_spacing) * tick_spacing; 
    let max_tick: i32 = (MAX_TICK / tick_spacing) * tick_spacing; 

    let num_ticks = ((max_tick - min_tick) / tick_spacing) as u32 + 1; 
    u128::MAX / num_ticks as u128
}


/// @notice Updates a tick and returns true if the tick was flipped from initialized to uninitialized, or vice versa
/// @param self The mapping containing all tick information for initialized ticks
/// @param tick The tick that will be updated
/// @param tickCurrent The current tick
/// @param liquidityDelta A new amount of liquidity to be added (subtracted) when tick is crossed from left to right (right to left)
/// @param feeGrowthGlobal0X128 The all-time global fee growth, per unit of liquidity, in token0
/// @param feeGrowthGlobal1X128 The all-time global fee growth, per unit of liquidity, in token1
/// @param secondsPerLiquidityCumulativeX128 The all-time seconds per max(1, liquidity) of the pool
/// @param tickCumulative The tick * time elapsed since the pool was first initialized
/// @param time The current block timestamp cast to a uint32
/// @param upper true for updating a position's upper tick, or false for updating a position's lower tick
/// @param maxLiquidity The maximum liquidity allocation for a single tick
/// @return flipped Whether the tick was flipped from initialized to uninitialized, or vice versa
pub fn _update (
    mapping: &mut HashMap<i32, Info>, 
    tick: i32, 
    liquidity_delta: i128,
    upper: bool, 
    max_liquidity: u128 
) -> Result<bool> {

    let tick_info = mapping.entry(tick).or_insert_with(Default::default);

    let liquidity_gross_before = tick_info.liquidity_gross; 
    let liquidity_gross_after = add_delta(liquidity_gross_before, liquidity_delta)?;

    if liquidity_gross_after > max_liquidity {return Err(eyre!("Liquidity gross larger than max liquidity"))}; 

    let flipped = (liquidity_gross_after == 0) != (liquidity_gross_before == 0); 

    if liquidity_gross_before == 0 {
        tick_info.initialized = true; 
    }

    tick_info.liquidity_gross = liquidity_gross_after; 
    if upper {
        match tick_info.liquidity_net.checked_sub(liquidity_delta) {
            Some(val) => {
                tick_info.liquidity_net = val; 
                Ok(flipped)
            },
            None => return Err(eyre!("Overflow occured in liquidity net calculation")) 
        }
    }  else {
        match tick_info.liquidity_net.checked_add(liquidity_delta) {
            Some(val) => {
                tick_info.liquidity_net = val; 
                Ok(flipped)
            },
            None => return Err(eyre!("Overflow occured in liquidity net calculation"))
        }
    }
}