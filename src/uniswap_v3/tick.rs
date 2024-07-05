use std::collections::HashMap;

use alloy::{primitives::{Address, U256}, sol, transports::http::{Client, Http}, providers::{Provider, ProviderBuilder, RootProvider}};
use super::{liquidity_math::add_delta, low_gas_safe_math::*, pool, tick_math::*}; 

#[derive(Default)]
pub struct Info {
    liquidity_gross: u128, 
    liquidity_net: i128, 
    fee_growth_outside0_x128: U256, 
    fee_growth_outside1_x128: U256, 
    tick_cumulative_outside: i64, 
    seconds_per_liquidity_outside_x128: U256, 
    seconds_outside: u32, 
    initialized: bool
}

/// @notice Derives max liquidity per tick from given tick spacing
/// @dev Executed within the pool constructor
/// @param tickSpacing The amount of required tick separation, realized in multiples of `tickSpacing`
///     e.g., a tickSpacing of 3 requires ticks to be initialized every 3rd tick i.e., ..., -6, -3, 0, 3, 6, ...
/// @return The max liquidity per tick
pub fn tick_spacing_to_max_liquidity_per_tick ( tick_spacing: i32 ) -> u128 {
    let min_tick: i32 = (MIN_TICK / tick_spacing) * tick_spacing; 
    let max_tick: i32 = (MAX_TICK / tick_spacing) * tick_spacing; 

    let num_ticks = ((max_tick - min_tick) / tick_spacing) as u32 + 1; 
    u128::MAX / num_ticks as u128
}

/// @notice Retrieves fee growth data
/// @param self The mapping containing all tick information for initialized ticks
/// @param tickLower The lower tick boundary of the position
/// @param tickUpper The upper tick boundary of the position
/// @param tickCurrent The current tick
/// @param feeGrowthGlobal0X128 The all-time global fee growth, per unit of liquidity, in token0
/// @param feeGrowthGlobal1X128 The all-time global fee growth, per unit of liquidity, in token1
/// @return feeGrowthInside0X128 The all-time fee growth in token0, per unit of liquidity, inside the position's tick boundaries
/// @return feeGrowthInside1X128 The all-time fee growth in token1, per unit of liquidity, inside the position's tick boundaries
pub async fn get_fee_growth_inside (
    provider: &RootProvider<Http<Client>>, 
    pool_address: Address,
    tick_lower: i32, 
    tick_upper: i32, 
    tick_current: i32, 
    fee_growth_global0_x128: U256, 
    fee_growth_global1_x128: U256
) -> Result<(U256, U256), String> {
    //let lower = mapping.get(&tick_lower).ok_or("Lower tick is not in the mapping".to_string())?;
    //let upper = mapping.get(&tick_upper).ok_or("Lower tick is not in the mapping".to_string())?;
    let lower = get_tick_info_from_ticks(provider, pool_address, &tick_lower).await?; 
    let upper = get_tick_info_from_ticks(provider, pool_address, &tick_upper).await?;


    let (fee_growth_below0_x128, fee_growth_below1_x128) = match tick_current >= tick_lower {
        true => (lower.fee_growth_outside0_x128, lower.fee_growth_outside1_x128), 
        false => (fee_growth_global0_x128 - lower.fee_growth_outside0_x128, fee_growth_global1_x128 - lower.fee_growth_outside1_x128)
    };

    let (fee_growth_above0_x128, fee_growth_above1_x128) = match tick_current < tick_upper {
        true => (upper.fee_growth_outside0_x128, upper.fee_growth_outside1_x128), 
        false => (fee_growth_global0_x128 - upper.fee_growth_outside0_x128, fee_growth_global1_x128 - upper.fee_growth_outside1_x128)
    };

    let fee_growth_inside0_x128 = fee_growth_global0_x128 - fee_growth_below0_x128 - fee_growth_above0_x128; 
    let fee_growth_inside1_x128 = fee_growth_global1_x128 - fee_growth_below1_x128 - fee_growth_above1_x128; 

    Ok((fee_growth_inside0_x128, fee_growth_inside1_x128))
}

pub async fn get_tick_info_from_ticks (provider: &RootProvider<Http<Client>>, pool_address: Address, tick: &i32) -> Result<Info, String> {
    sol! {
        #[sol(rpc)]
        interface IPool {
            function ticks(int24 tick)
            external
            view
            returns (
                uint128 liquidityGross,
                int128 liquidityNet,
                uint256 feeGrowthOutside0X128,
                uint256 feeGrowthOutside1X128,
                int56 tickCumulativeOutside,
                uint160 secondsPerLiquidityOutsideX128,
                uint32 secondsOutside,
                bool initialized
            );
        }
    }

    let pool = IPool::new(pool_address, provider); 
    let tick_info: Info = match pool.ticks(*tick).call().await.map_err(|e| e.to_string())? {
        IPool::ticksReturn{
            liquidityGross, 
            liquidityNet, 
            feeGrowthOutside0X128, 
            feeGrowthOutside1X128, 
            tickCumulativeOutside, 
            secondsPerLiquidityOutsideX128, 
            secondsOutside, 
            initialized
        } => Info {
            liquidity_gross: liquidityGross, 
            liquidity_net: liquidityNet, 
            fee_growth_outside0_x128: feeGrowthOutside0X128, 
            fee_growth_outside1_x128: feeGrowthOutside1X128, 
            tick_cumulative_outside: tickCumulativeOutside, 
            seconds_per_liquidity_outside_x128: secondsPerLiquidityOutsideX128, 
            seconds_outside: secondsOutside,
            initialized: initialized
        }
    };


    Ok(tick_info)
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
pub fn update (
    mapping: &mut HashMap<i32, Info>, 
    tick: i32, 
    tick_current: i32, 
    liquidity_delta: i128, 
    fee_growth_global0_x128: U256, 
    fee_growth_global1_x128: U256, 
    seconds_per_liquidity_cumulative_x128: U256, 
    tick_cumulative: i64,
    time: u32, 
    upper: bool, 
    max_liquidity: u128 
) -> Result<bool, String> {

    let tick_info = mapping.entry(tick).or_insert_with(Default::default);

    let liquidity_gross_before = tick_info.liquidity_gross; 
    let liquidity_gross_after = add_delta(liquidity_gross_before, liquidity_delta)?;

    if liquidity_gross_after > max_liquidity {return Err("Liquidity gross larger than max liquidity".to_string())}; 

    let flipped = (liquidity_gross_after == 0) != (liquidity_gross_before == 0); 

    if liquidity_gross_before == 0 {
        if tick <= tick_current {
            tick_info.fee_growth_outside0_x128 = fee_growth_global0_x128; 
            tick_info.fee_growth_outside1_x128 = fee_growth_global1_x128; 
            tick_info.seconds_per_liquidity_outside_x128 = seconds_per_liquidity_cumulative_x128; 
            tick_info.tick_cumulative_outside = tick_cumulative; 
            tick_info.seconds_outside = time;
        }; 

        tick_info.initialized = true; 
    }

    tick_info.liquidity_gross = liquidity_gross_after; 
    if upper {
        match tick_info.liquidity_net.checked_sub(liquidity_delta) {
            Some(val) => {
                tick_info.liquidity_net = val; 
                Ok(flipped)
            },
            None => return Err("Overflow occured in liquidity net calculation".to_string()) 
        }
    }  else {
        match tick_info.liquidity_net.checked_add(liquidity_delta) {
            Some(val) => {
                tick_info.liquidity_net = val; 
                Ok(flipped)
            },
            None => return Err("Overflow occured in liquidity net calculation".to_string())
        }
    }
}

/// @notice Clears tick data
/// @param self The mapping containing all initialized tick information for initialized ticks
/// @param tick The tick that will be cleared
pub fn clear(mapping: &mut HashMap<i32, Info>, tick: &i32) {
    mapping.remove(&tick); 
}

/// @notice Transitions to next tick as needed by price movement
/// @param self The mapping containing all tick information for initialized ticks
/// @param tick The destination tick of the transition
/// @param feeGrowthGlobal0X128 The all-time global fee growth, per unit of liquidity, in token0
/// @param feeGrowthGlobal1X128 The all-time global fee growth, per unit of liquidity, in token1
/// @param secondsPerLiquidityCumulativeX128 The current seconds per liquidity
/// @param tickCumulative The tick * time elapsed since the pool was first initialized
/// @param time The current block.timestamp
/// @return liquidityNet The amount of liquidity added (subtracted) when tick is crossed from left to right (right to left)
pub async fn cross(
    provider: &RootProvider<Http<Client>>, 
    pool_address: Address, 
    tick: i32,
) -> Result<i128, String> {
    let tick_info = get_tick_info_from_ticks(provider, pool_address, &tick).await?; 
    Ok(tick_info.liquidity_net)
}