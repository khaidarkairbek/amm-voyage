use alloy::primitives::U256;

use super::constants::U256_1;

/// @notice Returns ceil(x / y)
/// @dev division by 0 has unspecified behavior, and must be checked externally
/// @param x The dividend
/// @param y The divisor
/// @return z The quotient, ceil(x / y)
pub fn div_rounding_up(x: U256, y: U256) -> U256 {
    if x.wrapping_rem(y).is_zero() {
        x.wrapping_div(y)
    } else {
        x.wrapping_div(y) + U256_1
    }
}