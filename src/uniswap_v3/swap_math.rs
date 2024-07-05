use alloy::primitives::{U256, I256}; 
use super::{full_math, sqrt_price_math}; 


/// @notice Computes the result of swapping some amount in, or amount out, given the parameters of the swap
/// @dev The fee, plus the amount in, will never exceed the amount remaining if the swap's `amountSpecified` is positive
/// @param sqrtRatioCurrentX96 The current sqrt price of the pool
/// @param sqrtRatioTargetX96 The price that cannot be exceeded, from which the direction of the swap is inferred
/// @param liquidity The usable liquidity
/// @param amountRemaining How much input or output amount is remaining to be swapped in/out
/// @param feePips The fee taken from the input amount, expressed in hundredths of a bip
/// @return sqrtRatioNextX96 The price after swapping the amount in/out, not to exceed the price target
/// @return amountIn The amount to be swapped in, of either token0 or token1, based on the direction of the swap
/// @return amountOut The amount to be received, of either token0 or token1, based on the direction of the swap
/// @return feeAmount The amount of input that will be taken as a fee
pub fn compute_swap_step (
    sqrt_ratio_current_x96: U256, 
    sqrt_ratio_target_x96: U256, 
    liquidity: u128, 
    amount_remaining: I256, 
    fee_pips: u32
) -> Result<(U256, U256, U256, U256), String> {
    let zero_for_one = sqrt_ratio_current_x96 >= sqrt_ratio_target_x96; 
    let exact_in = amount_remaining >= I256::ZERO; 

    let mut amount_in = U256::ZERO; 
    let mut amount_out = U256::ZERO; 

    let sqrt_ratio_next_x96 = match exact_in {
        true => {
            let amount_remaining_less_fee = full_math::mul_div(U256::from(amount_remaining.into_raw()), U256::from(1e6 as u32 - fee_pips), U256::from_limbs([1000000, 0, 0, 0]))?; 
            amount_in = match zero_for_one {
                true => {
                    sqrt_price_math::get_amount0_delta_round_up(sqrt_ratio_target_x96, sqrt_ratio_current_x96, liquidity, true)
                }, 
                false => {
                    sqrt_price_math::get_amount1_delta_round_up(sqrt_ratio_current_x96, sqrt_ratio_target_x96, liquidity, true)
                }
            }?; 

            if amount_remaining_less_fee >= amount_in {
                sqrt_ratio_target_x96
            } else {
                sqrt_price_math::get_next_sqrt_price_from_input(sqrt_ratio_current_x96, liquidity, amount_remaining_less_fee, zero_for_one)?
            }
        }, 
        false => {
            amount_out = match zero_for_one {
                true => {
                    sqrt_price_math::get_amount1_delta_round_up(sqrt_ratio_target_x96, sqrt_ratio_current_x96, liquidity, false)
                }, 
                false => {
                    sqrt_price_math::get_amount0_delta_round_up(sqrt_ratio_current_x96, sqrt_ratio_target_x96, liquidity, false)
                }
            }?;

            if amount_remaining.unsigned_abs() >= amount_out {
                sqrt_ratio_target_x96
            } else {
                sqrt_price_math::get_next_sqrt_price_from_output(sqrt_ratio_current_x96, liquidity, amount_remaining.unsigned_abs(), zero_for_one)?
            }
        }
    }; 

    let max = sqrt_ratio_target_x96 == sqrt_ratio_next_x96; 

    if zero_for_one {
        if !(max && exact_in) {
            amount_in = sqrt_price_math::get_amount0_delta_round_up(sqrt_ratio_next_x96, sqrt_ratio_current_x96, liquidity, true)?;
        }

        if !(max && !exact_in) {
            amount_out = sqrt_price_math::get_amount1_delta_round_up(sqrt_ratio_next_x96, sqrt_ratio_current_x96, liquidity, false)?; 
        }

    } else {
        if !(max && exact_in) {
            amount_in = sqrt_price_math::get_amount1_delta_round_up(sqrt_ratio_current_x96, sqrt_ratio_next_x96, liquidity, true)?;
        }

        if !(max && !exact_in) {
            amount_out = sqrt_price_math::get_amount0_delta_round_up(sqrt_ratio_current_x96, sqrt_ratio_next_x96, liquidity, false)?; 
        }
    }; 

    if !exact_in && amount_out > amount_remaining.unsigned_abs() {
        amount_out = amount_remaining.unsigned_abs();
    }

    let fee_amount = if exact_in && sqrt_ratio_next_x96 != sqrt_ratio_target_x96 {
        amount_remaining.unsigned_abs() - amount_in 
    } else {
        full_math::mul_div_rounding_up(amount_in, U256::from(fee_pips), U256::from(1e6 as u32 - fee_pips))?
    }; 

    Ok((
        sqrt_ratio_next_x96, 
        amount_in, 
        amount_out, 
        fee_amount
    ))
}