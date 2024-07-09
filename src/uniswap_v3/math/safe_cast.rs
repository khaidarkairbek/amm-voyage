use alloy::primitives::{U256, I256};
use eyre::{eyre, Result}; 


pub fn to_int256 (
    a: U256
) -> Result<I256> {
    let b: I256 = I256::from_raw(a); 
    if b < I256::ZERO {
        return Err(eyre!("Overflow when converting from U256 to I256")); 
    }
    Ok(b)
}