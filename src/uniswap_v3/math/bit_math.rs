use alloy::primitives::U256; 
use eyre::{eyre, Result};

/// @notice Returns the index of the most significant bit of the number,
///     where the least significant bit is at index 0 and the most significant bit is at index 255
/// @dev The function satisfies the property:
///     x >= 2**mostSignificantBit(x) and x < 2**(mostSignificantBit(x)+1)
/// @param x the value for which to compute the most significant bit, must be greater than 0
/// @return r the index of the most significant bit
pub fn most_significant_bit (x: U256) -> Result<u8> {
    if x.is_zero() {
        return Err(eyre!("X can not be zero"))
    }

    Ok(255 - x.leading_zeros() as u8)
}


/// @notice Returns the index of the least significant bit of the number,
///     where the least significant bit is at index 0 and the most significant bit is at index 255
/// @dev The function satisfies the property:
///     (x & 2**leastSignificantBit(x)) != 0 and (x & (2**(leastSignificantBit(x)) - 1)) == 0)
/// @param x the value for which to compute the least significant bit, must be greater than 0
/// @return r the index of the least significant bit
pub fn least_significant_bits (x: U256) -> Result<u8> {
    if x.is_zero() {
        return Err(eyre!("X can not be zero"))
    }

    Ok(x.trailing_zeros() as u8)
}



