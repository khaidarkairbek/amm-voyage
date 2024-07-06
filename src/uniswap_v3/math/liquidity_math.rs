pub fn add_delta (x : u128, y : i128) -> Result<u128, String> {
    match y < 0 {
        true => {
            let z = x - y.unsigned_abs(); 
            if z < x {
                Ok(z)
            } else {
                Err("Add delta error: LS".to_string())
            }
        }, 
        false => {
            let z = x + y.unsigned_abs(); 
            if z >= x {
                Ok(z)
            } else {
                Err("Add delta error: LA".to_string())
            }
        }
    }
}

