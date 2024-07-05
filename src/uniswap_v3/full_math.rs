use alloy::primitives::U256; 
use super::constants::{U256_1, U256_2, U256_3};



/// @notice Calculates floor(a×b÷denominator) with full precision. Throws if result overflows a uint256 or denominator == 0
/// @param a The multiplicand
/// @param b The multiplier
/// @param denominator The divisor
/// @return result The 256-bit result
/// @dev Credit to Remco Bloemen under MIT license https://xn--2-umb.com/21/muldiv
pub fn mul_div(
    a: U256, 
    b: U256, 
    mut denominator: U256
) -> Result<U256, String> {
    // 512-bit multiply [prod1 prod0] = a * b
    // Compute the product mod 2**256 and mod 2**256 - 1
    // then use the Chinese Remainder Theorem to reconstruct
    // the 512 bit result. The result is stored in two 256
    // variables such that product = prod1 * 2**256 + prod0
    let mm = a.mul_mod(b, U256::MAX);

    let mut prod0 = a.overflowing_mul(b).0; 
    let mut prod1 = mm.overflowing_sub(prod0).0.overflowing_sub(U256::from(mm < prod0)).0; 

    // Handle non-overflow cases, 256 by 256 division
    if prod1.is_zero() {
        if denominator.is_zero() {
            return Err("Denominator is zero".to_string())
        } else {
            return Ok(prod0.wrapping_div(denominator))
        }
    } else {
        if denominator <= prod1 {
            return Err("Denomniator is less than prod one".to_string())
        } else {
            let remainder = a.mul_mod(b, denominator);
            prod0 = prod0.overflowing_sub(remainder).0; 
            prod1 = prod1.overflowing_sub(U256::from(remainder > prod0)).0;

            // Factor powers of two out of denominator
            // Compute largest power of two divisor of denominator.
            // Always >= 1.
            let mut twos = -denominator & denominator;

            denominator = denominator.wrapping_div(twos);
            // Divide [prod1 prod0] by the factors of two
            prod0 = prod0.wrapping_div(twos); 
            // Shift in bits from prod1 into prod0. For this we need
            // to flip `twos` such that it is 2**256 / twos.
            // If twos is zero, then it becomes one
            twos = U256::ZERO.overflowing_sub(twos).0.wrapping_div(twos).overflowing_add(U256_1).0;

            prod0 = prod0 | (prod1*twos);

            // Invert denominator mod 2**256
            // Now that denominator is an odd number, it has an inverse
            // modulo 2**256 such that denominator * inv = 1 mod 2**256.
            // Compute the inverse by starting with a seed that is correct
            // correct for four bits. That is, denominator * inv = 1 mod 2**4
            let mut inv = (U256_3 * denominator) ^ U256_2;

            // Now use Newton-Raphson iteration to improve the precision.
            // Thanks to Hensel's lifting lemma, this also works in modular
            // arithmetic, doubling the correct bits in each step.

            inv = inv * (U256_2 - denominator * inv); 
            inv = inv * (U256_2 - denominator * inv);
            inv = inv * (U256_2 - denominator * inv); 
            inv = inv * (U256_2 - denominator * inv); 
            inv = inv * (U256_2 - denominator * inv); 
            inv = inv * (U256_2 - denominator * inv); 

            // Because the division is now exact we can divide by multiplying
            // with the modular inverse of denominator. This will give us the
            // correct result modulo 2**256. Since the precoditions guarantee
            // that the outcome is less than 2**256, this is the final result.
            // We don't need to compute the high bits of the result and prod1
            // is no longer required.

            Ok(prod0*inv)
        }
    }
}

/// @notice Calculates ceil(a×b÷denominator) with full precision. Throws if result overflows a uint256 or denominator == 0
/// @param a The multiplicand
/// @param b The multiplier
/// @param denominator The divisor
/// @return result The 256-bit result
pub fn mul_div_rounding_up(
    a: U256, 
    b: U256, 
    denominator: U256
) -> Result<U256, String> {

    match mul_div(a, b, denominator) {
        Ok(mut result) => {
            if a.mul_mod(b, denominator) > U256::ZERO {
                if result < U256::MAX {
                    result = result + U256_1; 
                    Ok(result)
                } else {
                    Err("Result is u256 max value".to_string())
                }
            } else {
                Ok(result)
            }
        }, 
        Err(e) => Err(e)
    }
}