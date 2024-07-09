use alloy::primitives::U256;

use super::{bit_math::*, constants::U256_1, super::PoolState};
use eyre::{eyre, Result};

/// @notice Computes the position in the mapping where the initialized bit for a tick lives
/// @param tick The tick for which to compute the position
/// @return wordPos The key in the mapping containing the word in which the bit is stored
/// @return bitPos The bit position in the word where the flag is stored
pub fn position (tick: i32) -> (i16, u8) {
    let word_pos:i16 = (tick >> 8) as i16;
    let bit_pos: u8 = (tick % 256) as u8;

    (word_pos, bit_pos)
}

/// @notice Returns the next initialized tick contained in the same word (or adjacent word) as the tick that is either
/// to the left (less than or equal to) or right (greater than) of the given tick
/// @param self The mapping in which to compute the next initialized tick
/// @param tick The starting tick
/// @param tickSpacing The spacing between usable ticks
/// @param lte Whether to search for the next initialized tick to the left (less than or equal to the starting tick)
/// @return next The next initialized or uninitialized tick up to 256 ticks away from the current tick
/// @return initialized Whether the next tick is initialized, as the function only searches within up to 256 ticks
pub async fn next_initialized_tick_within_one_word (
    pool_state: &PoolState,
    tick: i32,
    lte: bool
) -> Result<(i32, bool)>{
    let mut compressed: i32 = tick / pool_state.tick_spacing;
    if tick < 0 && tick % pool_state.tick_spacing != 0 {
        compressed = compressed - 1; 
    }

    match lte {
        true => {
            let (word_pos, bit_pos) = position(compressed); 
            let mask: U256 = (U256_1 << bit_pos) - U256_1 + (U256_1 << bit_pos); 

            let word = match pool_state.tick_bitmap.get(&word_pos) {
                Some(word) => word, 
                None => return Err(eyre!("Word position not in tick bitmap"))            
            };
            //get_word_from_bitmap(provider, pool_address, &word_pos).await?; 
            let masked = word & mask;

            let initialized = !masked.is_zero();  
            match initialized {
                true => {
                    let next = (compressed - ((bit_pos - most_significant_bit(masked)?) as i32)) * pool_state.tick_spacing; 
                    Ok((next, initialized))
                }, 
                false => {
                    let next = (compressed - bit_pos as i32) * pool_state.tick_spacing; 
                    Ok((next, initialized))
                }
            }

        }, 
        false => {
            let (word_pos, bit_pos) = position(compressed + 1); 
            let mask: U256 = !((U256_1 << bit_pos) - U256_1); 
            let word = match pool_state.tick_bitmap.get(&word_pos) {
                Some(word) => word, 
                None => return Err(eyre!("Word position not in tick bitmap"))            
            };
            let masked = word & mask;
            let initialized = !masked.is_zero();
            match initialized {
                true => {
                    let next = (compressed + 1 + ((least_significant_bits(masked)? - bit_pos) as i32)) * pool_state.tick_spacing; 
                    Ok((next, initialized))
                }, 
                false => {
                    let next = (compressed + 1 + (u8::MAX - bit_pos) as i32) * pool_state.tick_spacing; 
                    Ok((next, initialized))
                }
            }
        }
    }

}
