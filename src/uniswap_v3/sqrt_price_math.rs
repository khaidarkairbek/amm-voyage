
use alloy::primitives::{U256, U160, I256};
use super::{full_math, low_gas_safe_math, unsafe_math, constants::{FIXED_POINT96_RESOLUTION, Q96}, safe_cast::to_int256}; 


/// @notice Gets the next sqrt price given a delta of token0
/// @dev Always rounds up, because in the exact output case (increasing price) we need to move the price at least
/// far enough to get the desired output amount, and in the exact input case (decreasing price) we need to move the
/// price less in order to not send too much output.
/// The most precise formula for this is liquidity * sqrtPX96 / (liquidity +- amount * sqrtPX96),
/// if this is impossible because of overflow, we calculate liquidity / (liquidity / sqrtPX96 +- amount).
/// @param sqrtPX96 The starting price, i.e. before accounting for the token0 delta
/// @param liquidity The amount of usable liquidity
/// @param amount How much of token0 to add or remove from virtual reserves
/// @param add Whether to add or remove the amount of token0
/// @return The price after adding or removing amount, depending on add
pub fn get_next_sqrt_price_from_amount0_rounding_up (
    sqrt_px96: U256, 
    liquidity: u128, 
    amount: U256, 
    add: bool
) -> Result<U256, String> {
    // we short circuit amount == 0 because the result is otherwise not guaranteed to equal the input price
    if amount.is_zero() {return Err("Amount is zero: no change".to_string())}; 

    let numerator1: U256 = U256::from(liquidity) << FIXED_POINT96_RESOLUTION; 

    match add {
        true => {
            let product = amount.wrapping_mul(sqrt_px96); 
            if product.wrapping_div(amount) == sqrt_px96 {
                let denominator = numerator1.wrapping_add(product); 
                return full_math::mul_div_rounding_up(numerator1, sqrt_px96, denominator)
            } else {
                match low_gas_safe_math::unsigned_add(numerator1.wrapping_div(sqrt_px96), amount) {
                    Ok(result) => Ok(unsafe_math::div_rounding_up(numerator1, result)),
                    Err(e) => return Err(e)
                }
            }
        },
        false => {
            let product = amount.wrapping_mul(sqrt_px96);
            if product.wrapping_div(amount) == sqrt_px96 && numerator1 > product {
                let denominator = numerator1.wrapping_sub(product);
                let result = full_math::mul_div_rounding_up(numerator1, sqrt_px96, denominator)?; 
                match result > U256::from(U160::MAX) {
                    true => {
                        Err("Sqrt price x96 is bigger than u160".to_string())
                    }, 
                    false => {
                        Ok(result)
                    }
                }
            } else {
                Err("Error getting sqrt price from amount 0 rounding up".to_string())
            }
        }
    }
}

/// @notice Gets the next sqrt price given a delta of token1
/// @dev Always rounds down, because in the exact output case (decreasing price) we need to move the price at least
/// far enough to get the desired output amount, and in the exact input case (increasing price) we need to move the
/// price less in order to not send too much output.
/// The formula we compute is within <1 wei of the lossless version: sqrtPX96 +- amount / liquidity
/// @param sqrtPX96 The starting price, i.e., before accounting for the token1 delta
/// @param liquidity The amount of usable liquidity
/// @param amount How much of token1 to add, or remove, from virtual reserves
/// @param add Whether to add, or remove, the amount of token1
/// @return The price after adding or removing `amount`
pub fn get_next_sqrt_price_from_amount1_rounding_down (
    sqrt_px96: U256, 
    liquidity: u128, 
    amount: U256, 
    add: bool
) -> Result<U256, String> {
    match add {
        true => {
            let quotient = match amount <= U256::from(U160::MAX) {
                true => {
                    let amount_shifted: U256 = amount << FIXED_POINT96_RESOLUTION; 
                    Ok((amount_shifted).wrapping_div(U256::from(liquidity)))
                }, 
                false => full_math::mul_div(amount, Q96, U256::from(liquidity))
            }?;
            
            let result = low_gas_safe_math::unsigned_add(sqrt_px96, quotient)?; 
            match result > U256::from(U160::MAX) {
                true => {
                    Err("Sqrt price x96 is bigger than u160".to_string())
                }, 
                false => {
                    Ok(result)
                }
            }
        }, 
        false => {
            let quotient = match amount <= U256::from(U160::MAX) {
                true => {
                    Ok(unsafe_math::div_rounding_up(amount << FIXED_POINT96_RESOLUTION, U256::from(liquidity)))
                }, 
                false => {
                    full_math::mul_div_rounding_up(amount, Q96, U256::from(liquidity))
                }
            }?;
            if sqrt_px96 > quotient {
                Ok(sqrt_px96.wrapping_sub(quotient))
            } else {
                Err("Price can not be lower than 0".to_string())
            }
        }
    }
}



/// @notice Gets the next sqrt price given an input amount of token0 or token1
/// @dev Throws if price or liquidity are 0, or if the next price is out of bounds
/// @param sqrtPX96 The starting price, i.e., before accounting for the input amount
/// @param liquidity The amount of usable liquidity
/// @param amountIn How much of token0, or token1, is being swapped in
/// @param zeroForOne Whether the amount in is token0 or token1
/// @return sqrtQX96 The price after adding the input amount to token0 or token1
pub fn get_next_sqrt_price_from_input(
    sqrt_px96: U256, 
    liquidity: u128, 
    amount_in: U256, 
    zero_for_one: bool
) -> Result<U256, String> {
    if sqrt_px96 > U256::ZERO && liquidity > 0 {
        match zero_for_one {
            true => get_next_sqrt_price_from_amount0_rounding_up(sqrt_px96, liquidity, amount_in, true), 
            false => get_next_sqrt_price_from_amount1_rounding_down(sqrt_px96, liquidity, amount_in, true)
        }
    } else {
        Err("Price and liquidity should be greater than zero".to_string())
    }
}

/// @notice Gets the next sqrt price given an output amount of token0 or token1
/// @dev Throws if price or liquidity are 0 or the next price is out of bounds
/// @param sqrtPX96 The starting price before accounting for the output amount
/// @param liquidity The amount of usable liquidity
/// @param amountOut How much of token0, or token1, is being swapped out
/// @param zeroForOne Whether the amount out is token0 or token1
/// @return sqrtQX96 The price after removing the output amount of token0 or token1
pub fn get_next_sqrt_price_from_output(
    sqrt_px96: U256, 
    liquidity: u128, 
    amount_out: U256, 
    zero_for_one: bool
) -> Result<U256, String> {
    if sqrt_px96 > U256::ZERO && liquidity > 0 {
        match zero_for_one {
            true => get_next_sqrt_price_from_amount1_rounding_down(sqrt_px96, liquidity, amount_out, false), 
            false => get_next_sqrt_price_from_amount0_rounding_up(sqrt_px96, liquidity, amount_out, false)
        }
    } else {
        Err("Price and liquidity should be greater than zero".to_string())
    }
}

/// @notice Gets the amount0 delta between two prices
/// @dev Calculates liquidity / sqrt(lower) - liquidity / sqrt(upper),
/// i.e. liquidity * (sqrt(upper) - sqrt(lower)) / (sqrt(upper) * sqrt(lower))
/// @param sqrtRatioAX96 A sqrt price
/// @param sqrtRatioBX96 Another sqrt price
/// @param liquidity The amount of usable liquidity
/// @param roundUp Whether to round the amount up or down
/// @return amount0 Amount of token0 required to cover a position of size liquidity between the two passed prices
pub fn get_amount0_delta_round_up (
    mut sqrt_ratio_ax96: U256, 
    mut sqrt_ratio_bx96: U256, 
    liquidity: u128, 
    round_up: bool
) -> Result<U256, String> {
    if sqrt_ratio_ax96 > sqrt_ratio_bx96 { 
        let temp = sqrt_ratio_ax96.clone();
        sqrt_ratio_ax96 = sqrt_ratio_bx96; 
        sqrt_ratio_bx96 = temp;
    }

    let numerator1: U256 = U256::from(liquidity) << FIXED_POINT96_RESOLUTION;
    let numerator2: U256 = sqrt_ratio_bx96.wrapping_sub(sqrt_ratio_ax96); 

    if sqrt_ratio_ax96.is_zero() {return Err("Sqrt ratio ax 96 can not be 0".to_string())}; 

    match round_up {
        true => {
            Ok(unsafe_math::div_rounding_up(full_math::mul_div_rounding_up(numerator1, numerator2, sqrt_ratio_bx96)?, sqrt_ratio_ax96))
        }, 
        false => {
            Ok(full_math::mul_div(numerator1, numerator2, sqrt_ratio_bx96)?.wrapping_div(sqrt_ratio_ax96))
        }
    }
}

/// @notice Gets the amount1 delta between two prices
/// @dev Calculates liquidity * (sqrt(upper) - sqrt(lower))
/// @param sqrtRatioAX96 A sqrt price
/// @param sqrtRatioBX96 Another sqrt price
/// @param liquidity The amount of usable liquidity
/// @param roundUp Whether to round the amount up, or down
/// @return amount1 Amount of token1 required to cover a position of size liquidity between the two passed prices
pub fn get_amount1_delta_round_up (
    mut sqrt_ratio_ax96: U256, 
    mut sqrt_ratio_bx96: U256, 
    liquidity: u128, 
    round_up: bool
) -> Result<U256, String> {
    if sqrt_ratio_ax96 > sqrt_ratio_bx96 { 
        std::mem::swap(&mut sqrt_ratio_ax96, &mut sqrt_ratio_bx96);
    }

    match round_up {
        true => {
            full_math::mul_div_rounding_up(U256::from(liquidity), sqrt_ratio_bx96.wrapping_sub(sqrt_ratio_ax96), Q96)
        }, 
        false => {
            full_math::mul_div(U256::from(liquidity), sqrt_ratio_bx96.wrapping_sub(sqrt_ratio_ax96), Q96)
        }
    }
}

/// @notice Helper that gets signed token0 delta
/// @param sqrtRatioAX96 A sqrt price
/// @param sqrtRatioBX96 Another sqrt price
/// @param liquidity The change in liquidity for which to compute the amount0 delta
/// @return amount0 Amount of token0 corresponding to the passed liquidityDelta between the two prices
pub fn get_amount0_delta(
    sqrt_ratio_ax96: U256, 
    sqrt_ratio_bx96: U256, 
    liquidity: i128,
) -> Result<I256, String> {
    match liquidity < 0 {
        true => Ok(-to_int256(get_amount0_delta_round_up(sqrt_ratio_ax96, sqrt_ratio_bx96, liquidity.unsigned_abs(), false)?)?), 
        false => to_int256(get_amount0_delta_round_up(sqrt_ratio_ax96, sqrt_ratio_bx96, liquidity.unsigned_abs(), true)?)
    }
}
/// @notice Helper that gets signed token1 delta
/// @param sqrtRatioAX96 A sqrt price
/// @param sqrtRatioBX96 Another sqrt price
/// @param liquidity The change in liquidity for which to compute the amount1 delta
/// @return amount1 Amount of token1 corresponding to the passed liquidityDelta between the two prices

pub fn get_amount1_delta(
    sqrt_ratio_ax96: U256, 
    sqrt_ratio_bx96: U256, 
    liquidity: i128,
) -> Result<I256, String> {
    match liquidity < 0 {
        true => Ok(-to_int256(get_amount1_delta_round_up(sqrt_ratio_ax96, sqrt_ratio_bx96, liquidity.unsigned_abs(), false)?)?), 
        false => to_int256(get_amount1_delta_round_up(sqrt_ratio_ax96, sqrt_ratio_bx96, liquidity.unsigned_abs(), true)?)
    }
}