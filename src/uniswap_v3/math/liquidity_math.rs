use eyre::{eyre, Result};

pub fn add_delta (x : u128, y : i128) -> Result<u128> {
    match y < 0 {
        true => {
            let z = x - y.unsigned_abs(); 
            if z < x {
                Ok(z)
            } else {
                Err(eyre!("Add delta error: LS"))
            }
        }, 
        false => {
            let z = x + y.unsigned_abs(); 
            if z >= x {
                Ok(z)
            } else {
                Err(eyre!("Add delta error: LA"))
            }
        }
    }
}

