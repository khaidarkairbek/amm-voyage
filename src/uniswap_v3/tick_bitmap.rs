use std::collections::HashMap;

use alloy::{primitives::{Address, U256}, sol, transports::http::{Client, Http}, providers::RootProvider};

use super::{constants::U256_1, bit_math::*}; 


/// @notice Computes the position in the mapping where the initialized bit for a tick lives
/// @param tick The tick for which to compute the position
/// @return wordPos The key in the mapping containing the word in which the bit is stored
/// @return bitPos The bit position in the word where the flag is stored
pub fn position (tick: i32) -> (i16, u8) {
    let word_pos:i16 = (tick >> 8) as i16;
    let bit_pos: u8 = (tick % 256) as u8;

    (word_pos, bit_pos)
}


/// @notice Flips the initialized state for a given tick from false to true, or vice versa
/// @param self The mapping in which to flip the tick
/// @param tick The tick to flip
/// @param tickSpacing The spacing between usable ticks
pub fn flip_tick (mapping: &mut HashMap<i16, U256>, tick: i32, tick_spacing: i32) -> Result<(), String>{
    match tick % tick_spacing == 0 {
        true => {
            let (word_pos, bit_pos) = position(tick);
            let mask: U256 = U256_1 << bit_pos;
            let bit_map = mapping.get(&word_pos);
            match bit_map {
                Some(a) => mapping.insert(word_pos, a ^ mask), 
                None => mapping.insert(word_pos, U256::ZERO ^ mask), 
            };
            Ok(())
        }, 
        false => Err("Not valid tick".to_string())
    }
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
    provider: &RootProvider<Http<Client>>, 
    pool_address: Address,
    tick: i32, 
    tick_spacing: i32, 
    lte: bool
) -> Result<(i32, bool), String>{
    let mut compressed: i32 = tick / tick_spacing;
    if tick < 0 && tick % tick_spacing != 0 {
        compressed = compressed - 1; 
    }

    match lte {
        true => {
            let (word_pos, bit_pos) = position(compressed); 
            let mask: U256 = (U256_1 << bit_pos) - U256_1 + (U256_1 << bit_pos); 

            let word = get_word_from_bitmap(provider, pool_address, &word_pos).await?; 
            let masked = word & mask;

            let initialized = !masked.is_zero();  
            match initialized {
                true => {
                    let next = (compressed - ((bit_pos - most_significant_bit(masked)?) as i32)) * tick_spacing; 
                    Ok((next, initialized))
                }, 
                false => {
                    let next = (compressed - bit_pos as i32) * tick_spacing; 
                    Ok((next, initialized))
                }
            }

        }, 
        false => {
            let (word_pos, bit_pos) = position(compressed + 1); 
            let mask: U256 = !((U256_1 << bit_pos) - U256_1); 
            let word = get_word_from_bitmap(provider, pool_address, &word_pos).await?; 
            let masked = word & mask;
            let initialized = !masked.is_zero();
            match initialized {
                true => {
                    let next = (compressed + 1 + ((least_significant_bits(masked)? - bit_pos) as i32)) * tick_spacing; 
                    Ok((next, initialized))
                }, 
                false => {
                    let next = (compressed + 1 + (u8::MAX - bit_pos) as i32) * tick_spacing; 
                    Ok((next, initialized))
                }
            }
        }
    }

}


pub async fn get_word_from_bitmap (provider: &RootProvider<Http<Client>>, pool_address: Address, word_pos: &i16) -> Result<U256, String> {
    sol! {
        #[sol(rpc)]
        interface IPool {
            function tickBitmap(int16 wordPosition) external view returns (uint256);
        }
    }

    let pool = IPool::new(pool_address, provider); 
    let word = pool.tickBitmap(*word_pos).call().await.map_err(|e| e.to_string())?._0; 
    Ok(word)
}