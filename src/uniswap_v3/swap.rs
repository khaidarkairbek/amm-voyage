use std::{cmp::Ordering, iter};

use alloy::{
    primitives::{U256, I256}, 
    transports::http::{Client, Http}, 
    providers::RootProvider
};
use super::{math::constants::{U256_1, U256_2}, pool::{self, PoolState}};
use super::math::{liquidity_math, low_gas_safe_math, safe_cast, swap_math, tick_bitmap, tick_math};
use eyre::{eyre, Result};

pub struct SwapState {
    // the amount remaining to be swapped in/out of the input/output asset
    amount_specified_remaining: I256,
    // the amount already swapped out/in of the output/input asset
    amount_calculated: I256,
    // current sqrt(price)
    sqrt_price_x96: U256, 
    // the tick associated with the current price
    tick: i32,
    // the current liquidity in range
    liquidity: u128
}
#[derive(Default)]
pub struct StepComputations {
    // the price at the beginning of the step
    sqrt_price_start_x96: U256, 
    // the next tick to swap to from the current tick in the swap direction
    tick_next: i32,
    // whether tickNext is initialized or not
    initialized: bool, 
    // sqrt(price) for the next tick (1/0)
    sqrt_price_next_x96: U256, 
    // how much is being swapped in in this step
    amount_in: U256,
    // how much is being swapped out
    amount_out: U256,
    // how much fee is being paid in
    fee_amount: U256
}

pub async fn swap (
    provider: &RootProvider<Http<Client>>, 
    pool_state: &mut PoolState,
    zero_for_one: bool, 
    amount_specified: I256, 
    sqrt_price_limit_x96: U256
) -> Result<(I256, I256)>{
    if amount_specified == I256::ZERO {
        return Err(eyre!("Amount specified is zero, no swap"))
    }

    let slot0_start = &pool_state.slot0; 

    if !slot0_start.unlocked {
        return Err(eyre!("Pool is locked"))
    }
 
    if zero_for_one {
        if !(sqrt_price_limit_x96 < slot0_start.sqrt_price_x96 && sqrt_price_limit_x96 > tick_math::MIN_SQRT_RATIO) {
            return Err(eyre!("SPL"))
        }
    } else {
        if !(sqrt_price_limit_x96 > slot0_start.sqrt_price_x96 && sqrt_price_limit_x96 < tick_math::MAX_SQRT_RATIO) {
            return Err(eyre!("SPL"))
        }
    }

    let exact_input = amount_specified > I256::ZERO;

    let mut state:SwapState = SwapState {
        amount_specified_remaining: amount_specified, 
        amount_calculated: I256::ZERO, 
        sqrt_price_x96: slot0_start.sqrt_price_x96, 
        tick: slot0_start.tick,
        liquidity: pool_state.liquidity
    }; 

    while state.amount_specified_remaining != I256::ZERO && state.sqrt_price_x96 != sqrt_price_limit_x96 {
        let mut step: StepComputations = Default::default(); 
        step.sqrt_price_start_x96 = state.sqrt_price_x96; 
        (step.tick_next, step.initialized) = tick_bitmap::next_initialized_tick_within_one_word( pool_state, provider,  state.tick, zero_for_one).await?;

        if step.tick_next < tick_math::MIN_TICK {
            step.tick_next = tick_math::MIN_TICK;
        } else if step.tick_next > tick_math::MAX_TICK {
            step.tick_next = tick_math::MAX_TICK;
        }


        step.sqrt_price_next_x96 = tick_math::get_sqrt_ratio_at_tick(step.tick_next)?; 

        (state.sqrt_price_x96, step.amount_in, step.amount_out, step.fee_amount) = swap_math::compute_swap_step(
            state.sqrt_price_x96, 
            if zero_for_one {
                if step.sqrt_price_next_x96 < sqrt_price_limit_x96 {
                    sqrt_price_limit_x96
                } else {
                    step.sqrt_price_next_x96
                }
            } else {
                if step.sqrt_price_next_x96 > sqrt_price_limit_x96 {
                    sqrt_price_limit_x96
                } else {
                    step.sqrt_price_next_x96
                }
            }, 
            state.liquidity, 
            state.amount_specified_remaining, 
            pool_state.fee
        )?;

        if exact_input {
            state.amount_specified_remaining -= safe_cast::to_int256(step.amount_in + step.fee_amount)?;
            state.amount_calculated = low_gas_safe_math::signed_sub(state.amount_calculated, safe_cast::to_int256(step.amount_out)?)?; 
        } else {
            state.amount_specified_remaining += safe_cast::to_int256(step.amount_out)?; 
            state.amount_calculated = low_gas_safe_math::signed_add(state.amount_calculated, safe_cast::to_int256(step.amount_in + step.fee_amount)?)?;
        }

        if state.sqrt_price_x96 == step.sqrt_price_next_x96 {
            if step.initialized {
                let mut liquidity_net: i128 = match pool_state.ticks.get(&step.tick_next) {
                    Some(val) => val.liquidity_net, 
                    None => {
                        println!("Tick {} out of range: loading new liquidity map", step.tick_next); 
                        pool_state.update_ticks(provider, step.tick_next).await?; 
                        pool_state.ticks.get(&step.tick_next).ok_or(eyre!("Next tick out of allowed range"))?.liquidity_net
                    }
                };

                if zero_for_one {liquidity_net = -liquidity_net} 
                state.liquidity = liquidity_math::add_delta(state.liquidity, liquidity_net)?;
            }

            state.tick = if zero_for_one {step.tick_next - 1} else {step.tick_next};
        } else if state.sqrt_price_x96 != step.sqrt_price_start_x96 {
            state.tick = tick_math::get_tick_at_sqrt_ratio(state.sqrt_price_x96)?; 
        }
    }

    if zero_for_one == exact_input {
        Ok((amount_specified - state.amount_specified_remaining, state.amount_calculated))
    } else {
        Ok((state.amount_calculated, amount_specified - state.amount_specified_remaining))
    }
}

pub async fn swap_price_impact (
    provider: &RootProvider<Http<Client>>, 
    pool_state: &mut PoolState,
    price_impact: i32
) -> Result<(I256, I256)>{

    if price_impact < -100 {
        return Err(eyre!("Price impact less than -100%"))
    } else if price_impact == 0 {
        return Err(eyre!("No swap needed for 0% impact"))
    }

    let zero_for_one = price_impact < 0; 

    let amount_specified = I256::MAX;

    let sqrt_price_limit_x96 = calc_sqrt_price_limit_from_price_impact(pool_state.slot0.sqrt_price_x96, price_impact)?; 

    println!("Initial price {}, price limit {}", pool_state.slot0.sqrt_price_x96, sqrt_price_limit_x96); 

    let slot0_start = &pool_state.slot0; 

    if !slot0_start.unlocked {
        return Err(eyre!("Pool is locked"))
    }

    let exact_input = amount_specified > I256::ZERO;

    let mut state:SwapState = SwapState {
        amount_specified_remaining: amount_specified, 
        amount_calculated: I256::ZERO, 
        sqrt_price_x96: slot0_start.sqrt_price_x96, 
        tick: slot0_start.tick,
        liquidity: pool_state.liquidity
    }; 

    while state.amount_specified_remaining != I256::ZERO && state.sqrt_price_x96 != sqrt_price_limit_x96 {
        let mut step: StepComputations = Default::default(); 
        step.sqrt_price_start_x96 = state.sqrt_price_x96; 
        (step.tick_next, step.initialized) = tick_bitmap::next_initialized_tick_within_one_word( pool_state, provider, state.tick, zero_for_one).await?;

        if step.tick_next < tick_math::MIN_TICK {
            step.tick_next = tick_math::MIN_TICK;
        } else if step.tick_next > tick_math::MAX_TICK {
            step.tick_next = tick_math::MAX_TICK;
        }


        step.sqrt_price_next_x96 = tick_math::get_sqrt_ratio_at_tick(step.tick_next)?; 

        (state.sqrt_price_x96, step.amount_in, step.amount_out, step.fee_amount) = swap_math::compute_swap_step(
            state.sqrt_price_x96, 
            if zero_for_one {
                if step.sqrt_price_next_x96 < sqrt_price_limit_x96 {
                    sqrt_price_limit_x96
                } else {
                    step.sqrt_price_next_x96
                }
            } else {
                if step.sqrt_price_next_x96 > sqrt_price_limit_x96 {
                    sqrt_price_limit_x96
                } else {
                    step.sqrt_price_next_x96
                }
            }, 
            state.liquidity, 
            state.amount_specified_remaining, 
            pool_state.fee
        )?;
        println!("Current sqrt_price = {}", state.sqrt_price_x96);

        if exact_input {
            state.amount_specified_remaining -= safe_cast::to_int256(step.amount_in + step.fee_amount)?;
            state.amount_calculated = low_gas_safe_math::signed_sub(state.amount_calculated, safe_cast::to_int256(step.amount_out)?)?; 
        } else {
            state.amount_specified_remaining += safe_cast::to_int256(step.amount_out)?; 
            state.amount_calculated = low_gas_safe_math::signed_add(state.amount_calculated, safe_cast::to_int256(step.amount_in + step.fee_amount)?)?;
        }

        if state.sqrt_price_x96 == step.sqrt_price_next_x96 {
            if step.initialized {
                let mut liquidity_net: i128 = match pool_state.ticks.get(&step.tick_next) {
                    Some(val) => val.liquidity_net, 
                    None => {
                        println!("Tick {} out of range: loading new liquidity map", step.tick_next); 
                        pool_state.update_ticks(provider, step.tick_next).await?; 
                        pool_state.ticks.get(&step.tick_next).ok_or(eyre!("Next tick out of allowed range"))?.liquidity_net
                    }
                };

                if zero_for_one {liquidity_net = -liquidity_net} 
                state.liquidity = liquidity_math::add_delta(state.liquidity, liquidity_net)?;
            }

            state.tick = if zero_for_one {step.tick_next - 1} else {step.tick_next};
        } else if state.sqrt_price_x96 != step.sqrt_price_start_x96 {
            state.tick = tick_math::get_tick_at_sqrt_ratio(state.sqrt_price_x96)?; 
        }
    }

    if zero_for_one == exact_input {
        Ok((amount_specified - state.amount_specified_remaining, state.amount_calculated))
    } else {
        Ok((state.amount_calculated, amount_specified - state.amount_specified_remaining))
    }
}

pub fn calc_sqrt_price_limit_from_price_impact(
    sqrt_price_x96: U256, 
    price_impact: i32, 
) -> Result<U256> {
    if price_impact < 0 {
        Ok(sqrt(U256::from(1000000*(100 + price_impact)))? * sqrt_price_x96 / U256::from(10000))
    } else {
        Ok(sqrt(U256::from(1000000)*U256::from(100 + price_impact))? * sqrt_price_x96 / U256::from(10000))
    }
}

// Newton-Raphson Iteration to perform square-root operation on U256
// x (n+1) = x(n) - f(xn) / f'(xn) => finding roots is equivalent to finding roots of f(x) = x^2 - a
pub fn sqrt(a: U256) -> Result<U256>{
    match a.cmp(&U256::ZERO) {
        Ordering::Less => Err(eyre!("Sqrt calculation for positive values only")), 
        Ordering::Equal => Ok(a), 
        Ordering::Greater => {
            let mut xn = a; 
            let mut iter_count = 0; 
            loop {
                let squared = xn.pow(U256_2); 
                let xm = xn - (squared - a) / (U256_2 * xn);
                if xm == xn {
                    match a.cmp(&squared) {
                        Ordering::Less => return Ok(xn - U256_1), 
                        Ordering::Equal => return Ok(xn), 
                        Ordering::Greater => return Ok(xn + U256_1)
                    }
                } else if iter_count > 1000{
                    return Err(eyre!("Sqrt calculation didn't converge"))
                } else {
                    xn = xm; 
                    iter_count += 1; 
                }
            }
        }
    }
}


#[cfg(test)]
mod tests {
    use alloy::primitives::U160;

    use super::*;

    #[test]
    fn sqrt_test() {

        let a = U256::from(529);
        assert_eq!(sqrt(a).unwrap(), U256::from(23)); 

        let a = U256::from(100);
        assert_eq!(sqrt(a).unwrap(), U256::from(10)); 

        let a = U256::from(1000);
        assert_eq!(sqrt(a).unwrap(), U256::from(31)); 

        let a = U256::from(264257536);
        assert_eq!(sqrt(a).unwrap(), U256::from(16256)); 

        let a = U256::from(103698759);
        assert_eq!(sqrt(a).unwrap(), U256::from(10183)); 

        let a = U256::from(1);
        assert_eq!(sqrt(a).unwrap(), U256::from(1));
    }

    #[test]
    fn calc_sqrt_price_limit_test() {
        let sqrt_price_x96 = U256::from(1000000); 
        assert_eq!(calc_sqrt_price_limit_from_price_impact(sqrt_price_x96, -10).unwrap(), U256::from(948600)); 

        let sqrt_price_x96 = U256::from(U160::MAX); 
        assert_eq!(calc_sqrt_price_limit_from_price_impact(sqrt_price_x96, -100).unwrap(), U256::from(0)); 

        let sqrt_price_x96 = U256::from(1000000); 
        assert_eq!(calc_sqrt_price_limit_from_price_impact(sqrt_price_x96, 100000).unwrap(), U256::from(31638500)); 
    }
}