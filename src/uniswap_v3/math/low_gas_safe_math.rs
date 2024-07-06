use alloy::primitives::{U256, I256}; 

/// @notice Returns x + y, reverts if sum overflows uint256
/// @param x The augend
/// @param y The addend
/// @return z The sum of x and y
pub fn unsigned_add(x: U256, y: U256) -> Result<U256, String> {
    let (z, overflow) = x.overflowing_add(y); 
    match overflow {
        true => Err("Addition overflow".to_string()), 
        false => Ok(z)
    }
}

/// @notice Returns x - y, reverts if underflows
/// @param x The minuend
/// @param y The subtrahend
/// @return z The difference of x and y
pub fn _unsigned_sub(x: U256, y: U256) -> Result<U256, String> {
    let (z, underflow) = x.overflowing_sub(y); 
    match underflow {
        true => Err("Subtraction underflow".to_string()), 
        false => Ok(z)
    }
}

/// @notice Returns x * y, reverts if overflows
/// @param x The multiplicand
/// @param y The multiplier
/// @return z The product of x and y
pub fn _mul(x: U256, y: U256) -> Result<U256, String> {
    let (z, overflow) = x.overflowing_mul(y); 
    match overflow {
        true => Err("Multiplication overflow".to_string()), 
        false => Ok(z)
    }
}

/// @notice Returns x + y, reverts if overflows or underflows
/// @param x The augend
/// @param y The addend
/// @return z The sum of x and y
pub fn signed_add(x: I256, y: I256) -> Result<I256, String> {
    let (z, overflow) = x.overflowing_add(y); 
    match overflow {
        true => Err("Addition overflow".to_string()), 
        false => Ok(z)
    }
}

/// @notice Returns x - y, reverts if overflows or underflows
/// @param x The minuend
/// @param y The subtrahend
/// @return z The difference of x and y
pub fn signed_sub(x: I256, y: I256) -> Result<I256, String> {
    let (z, underflow) = x.overflowing_sub(y); 
    match underflow {
        true => Err("Subtraction underflow".to_string()), 
        false => Ok(z)
    }
}