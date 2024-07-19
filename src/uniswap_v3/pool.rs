use alloy::{ 
    primitives::{Address, Bytes, U256}, 
    providers::RootProvider, 
    sol, 
    sol_types::SolCall, 
    transports::http::{Client, Http}
};
use super::{math::{
    constants::{Q128, Q96, U256_2}, 
    full_math::{self, mul_div}, 
    tick::{get_fee_growth_inside, Info}, 
    tick_math::{MAX_SQRT_RATIO, MAX_TICK, MAX_WORD_POS, MIN_SQRT_RATIO, MIN_TICK, MIN_WORD_POS}
}, swap::sqrt};
use std::collections::HashMap; 
use eyre::{eyre, Result}; 
use super::{multicall::multicall, swap, math};
use polars::{prelude::*, io::prelude::CsvWriter}; 
use std::fs::File;

sol! {
    #[sol(rpc)]
    interface IPoolFactory {
        function getPool(
            address tokenA,
            address tokenB,
            uint24 fee
        ) external view returns (address pool);
    }
}

sol! {
    #[sol(rpc)]
    interface IPool {
        function slot0()
        external
        view
        returns (
            uint160 sqrtPriceX96,
            int24 tick,
            uint16 observationIndex,
            uint16 observationCardinality,
            uint16 observationCardinalityNext,
            uint8 feeProtocol,
            bool unlocked
        );

        function ticks(int24 tick)
            external
            view
            returns (
                uint128 liquidityGross,
                int128 liquidityNet,
                uint256 feeGrowthOutside0X128,
                uint256 feeGrowthOutside1X128,
                int56 tickCumulativeOutside,
                uint160 secondsPerLiquidityOutsideX128,
                uint32 secondsOutside,
                bool initialized
            );

        function tickSpacing() external view returns (int24);

        function factory() external view returns (address);

        function token0() external view returns (address);

        function token1() external view returns (address);

        function fee() external view returns (uint24);

        function liquidity() external view returns (uint128);

        function maxLiquidityPerTick() external view returns (uint128);

        function feeGrowthGlobal0X128() external view returns (uint256);

        function feeGrowthGlobal1X128() external view returns (uint256);

        function tickBitmap(int16 wordPosition) external view returns (uint256);
    }
}

sol! {
    #[sol(rpc)]
    interface IERC20 {

        function decimals() external view returns (uint8);

        function symbol() external view returns (string memory);
    }
}

pub struct Slot0 {
    pub sqrt_price_x96: U256,
    pub tick: i32,
    pub unlocked: bool
}

pub struct Token {
    pub address: Address, 
    pub symbol: String, 
    pub decimals: u8
}

pub struct PoolState {
    pub pool_address: Address,
    pub tick_spacing: i32, 
    pub fee: u32, 
    pub fee_growth_global0_x128: U256, 
    pub fee_growth_global1_x128: U256, 
    pub token0: Token, 
    pub token1: Token, 
    pub tick_bitmap: HashMap<i16, U256>, 
    pub slot0: Slot0, 
    pub liquidity: u128,
    pub ticks: HashMap<i32, Info>
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct SwapResult {
    pub amount_in: U256, 
    pub amount_out: U256
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct SwapResultSlippage {
    pub amount_in: U256, 
    pub amount_out: U256, 
    pub price_impact: U256,
}

pub enum LoadingPattern {
    LOW, 
    HIGH, 
    MID, 
    FULL
}

impl PoolState {
    pub async fn load (
        provider: &RootProvider<Http<Client>>,
        pool_factory_address: Address, 
        pair: (Address, Address),
        fee: u32, 
        loading_pattern: LoadingPattern
    ) -> Result<Self> {
        let pool_address = get_pool_address(provider, pool_factory_address, pair, fee).await?;
        println!("Pool address {}",pool_address);
    
        let (slot0, tick_spacing, liquidity, fee, token0_address, token1_address, fee_growth_global0_x128, fee_growth_global1_x128) = {
    
            let encoded_calls = vec![
                IPool::slot0Call{}.abi_encode(), 
                IPool::tickSpacingCall{}.abi_encode(), 
                IPool::liquidityCall{}.abi_encode(), 
                IPool::feeCall{}.abi_encode(), 
                IPool::token0Call{}.abi_encode(), 
                IPool::token1Call{}.abi_encode(), 
                IPool::feeGrowthGlobal0X128Call{}.abi_encode(),
                IPool::feeGrowthGlobal1X128Call{}.abi_encode(),
            ]; 
    
            let encoded_return_data: Vec<Bytes> = multicall(provider, pool_address, true, encoded_calls).await?
            .into_iter()
            .map(|result| {
                result.returnData
            })
            .collect();
    
            let slot0 = match IPool::slot0Call::abi_decode_returns(&encoded_return_data[0], true)? {
                IPool::slot0Return {
                    sqrtPriceX96, 
                    tick,
                    unlocked,..
                } => {
                    Slot0 {
                        sqrt_price_x96: sqrtPriceX96,
                        tick: tick,
                        unlocked: unlocked
                    }
                }
            };
    
            (
                slot0,
                IPool::tickSpacingCall::abi_decode_returns(&encoded_return_data[1], true)?._0, 
                IPool::liquidityCall::abi_decode_returns(&encoded_return_data[2], true)?._0, 
                IPool::feeCall::abi_decode_returns(&encoded_return_data[3], true)?._0, 
                IPool::token0Call::abi_decode_returns(&encoded_return_data[4], true)?._0,
                IPool::token1Call::abi_decode_returns(&encoded_return_data[5], true)?._0, 
                IPool::feeGrowthGlobal0X128Call::abi_decode_returns(&encoded_return_data[6], true)?._0,
                IPool::feeGrowthGlobal1X128Call::abi_decode_returns(&encoded_return_data[7], true)?._0,
            )
        };

        let token0_contract = IERC20::new(token0_address, provider);

        let token0 = Token {
            address: token0_address, 
            symbol: token0_contract.symbol().call().await?._0, 
            decimals: token0_contract.decimals().call().await?._0,
        }; 

        let token1_contract = IERC20::new(token1_address, provider); 

        let token1 = Token {
            address: token1_address, 
            symbol: token1_contract.symbol().call().await?._0, 
            decimals: token1_contract.decimals().call().await?._0,
        };

        let mut compressed: i32 = slot0.tick / tick_spacing;
        if slot0.tick < 0 && slot0.tick % tick_spacing != 0 {
            compressed = compressed - 1; 
        }
        let word_pos = (compressed >> 8) as i16;
    
        let ticks: HashMap<i32, Info> = Self::get_ticks(
            provider, 
            pool_address, 
            slot0.tick, 
            tick_spacing, 
            &loading_pattern
        ).await?;
    
        let tick_bitmap: HashMap<i16, U256> = Self::get_tick_bitmap(
            provider, 
            pool_address, 
            word_pos, 
            &loading_pattern
        ).await?; 
    
        Ok(PoolState{
            pool_address, 
            tick_spacing, 
            fee, 
            token0, 
            token1, 
            tick_bitmap, 
            slot0, 
            liquidity, 
            ticks, 
            fee_growth_global0_x128, 
            fee_growth_global1_x128
        })
    }

    pub async fn get_ticks (
        provider: &RootProvider<Http<Client>>,
        pool_address: Address ,
        tick: i32, 
        tick_spacing: i32, 
        load: &LoadingPattern 
    ) -> Result<HashMap<i32, Info>>{
        let compressed = tick / tick_spacing; 
        let min_compressed = MIN_TICK / tick_spacing; 
        let max_compressed = MAX_TICK / tick_spacing; 

        let tick_list: Vec<i32> = match load {
            LoadingPattern::MID => {
                let bottom = if min_compressed > compressed - 100 {min_compressed} else {compressed - 100};
                let top = if max_compressed < compressed + 100 {max_compressed} else {compressed + 100};
                bottom ..= top
            },
            LoadingPattern::HIGH => {
                let bottom = compressed; 
                let top = if max_compressed < compressed + 200 {max_compressed} else {compressed + 200};
                bottom ..= top
            },
            LoadingPattern::LOW => {
                let bottom = if min_compressed > compressed - 200 {min_compressed} else {compressed - 200}; 
                let top = compressed;
                bottom ..= top
            },
            LoadingPattern::FULL => {
                min_compressed ..= max_compressed
            }
        }.map(|compressed| compressed * tick_spacing).collect();

        let liqudity_tickmap_call_data: Vec<Vec<u8>> = tick_list
        .iter()
        .map(|&tick| {
            IPool::ticksCall{tick: tick}.abi_encode()
        })
        .collect(); 
        
        let return_data = multicall(provider, pool_address, false, liqudity_tickmap_call_data).await?;

        let mut map = HashMap::new();
        for (tick, data) in tick_list.into_iter().zip(return_data.iter()) {
            let info: Info = match IPool::ticksCall::abi_decode_returns(&data.returnData, true)? {
                IPool::ticksReturn{
                    liquidityGross, 
                    liquidityNet, 
                    feeGrowthOutside0X128, 
                    feeGrowthOutside1X128, 
                    initialized, ..
                } => {
                    Info {
                        liquidity_gross : liquidityGross, 
                        liquidity_net: liquidityNet, 
                        fee_growth_outside0_x128: feeGrowthOutside0X128, 
                        fee_growth_outside1_x128: feeGrowthOutside1X128, 
                        initialized: initialized
                    }
                }
            };
            map.insert(tick, info);
        } 

        Ok(map)
    }

    pub async fn update_ticks (
        &mut self,
        provider: &RootProvider<Http<Client>>, 
        next_tick: i32
    ) -> Result<()> {
        let load = if next_tick < self.slot0.tick {
            LoadingPattern::LOW
        } else {
            LoadingPattern::HIGH
        }; 

        self.ticks = Self::get_ticks(provider, self.pool_address, next_tick, self.tick_spacing, &load).await?;
        Ok(())
    }

    pub async fn get_tick_bitmap (
        provider: &RootProvider<Http<Client>>,
        pool_address: Address ,
        word_pos: i16,
        load: &LoadingPattern 
    ) -> Result<HashMap<i16, U256>>{ 
        // Generate word position list for tick bitmap
        let word_pos_list: Vec<i16> = match load {
            LoadingPattern::MID => {
                let bottom = if MIN_WORD_POS > word_pos - 20 {MIN_WORD_POS} else {word_pos - 20}; 
                let top = if MAX_WORD_POS < word_pos + 20 {MAX_WORD_POS} else {word_pos + 20};
                bottom ..= top
            }, 
            LoadingPattern::LOW => {
                let bottom = if MIN_WORD_POS > word_pos - 20 {MIN_WORD_POS} else {word_pos - 20}; 
                let top = word_pos; 
                bottom ..= top
            }, 
            LoadingPattern::HIGH => {
                let bottom = word_pos; 
                let top = if MAX_WORD_POS < word_pos + 20 {MAX_WORD_POS} else {word_pos + 20}; 
                bottom ..= top
            }, 
            LoadingPattern::FULL => {
                MIN_WORD_POS ..= MAX_WORD_POS
            }
        }.collect();

        let tick_bitmap_call_data: Vec<Vec<u8>> = word_pos_list
        .iter()
        .map(|&word_pos| {
            IPool::tickBitmapCall{wordPosition: word_pos}.abi_encode()
        })
        .collect(); 

        let return_data = multicall(provider, pool_address, false, tick_bitmap_call_data).await?;

        let mut map = HashMap::new();
        for (tick, data) in word_pos_list.into_iter().zip(return_data.iter()) {
            let word = IPool::tickBitmapCall::abi_decode_returns(&data.returnData, true)?._0; 
            map.insert(tick, word);
        } 

        Ok(map)
    }

    pub async fn update_tick_bitmap (
        &mut self,
        provider: &RootProvider<Http<Client>>, 
        word_pos: i16
    ) -> Result<()> {

        let init_tick = self.slot0.tick; 
        let mut compressed: i32 = init_tick / self.tick_spacing;
        if init_tick < 0 && init_tick % self.tick_spacing != 0 {
            compressed = compressed - 1;
        }

        let load = if word_pos < (compressed >> 8) as i16 {
            LoadingPattern::LOW
        } else {
            LoadingPattern::HIGH
        }; 

        self.tick_bitmap = Self::get_tick_bitmap(provider, self.pool_address, word_pos, &load).await?;
        Ok(())
    }

    pub fn export_to_df(
        &self
    ) -> Result<DataFrame> {
        let ticks = &self.ticks; 

        let mut tick = Vec::<i32>::new(); 
        let mut liquidity_net = Vec::<String>::new(); 
        let mut liquidity_gross = Vec::<String>::new(); 
        let mut fee_inside0 = Vec::<String>::new(); 
        let mut fee_inside1 = Vec::<String>::new(); 

        let token0_decimals = self.token0.decimals as u32; 
        let token1_decimals = self.token1.decimals as u32; 

        let liquidity_decimals = (token0_decimals + token1_decimals) / 2;
        println!("{}", liquidity_decimals);

        for (_tick, info) in ticks.iter() {
            tick.push(*_tick); 
            liquidity_net.push(info.liquidity_net.to_string()); 
            liquidity_gross.push(info.liquidity_gross.to_string()); 
            let lower_tick = _tick; 
            let upper_tick = _tick + self.tick_spacing; 
            let (fee_growth_inside0_x128, fee_growth_inside1_x128) = match ticks.get(&upper_tick) {
                Some(upper_info) => {
                    get_fee_growth_inside(
                        lower_tick, 
                        &upper_tick, 
                        info, 
                        upper_info, 
                        &self.slot0.tick, 
                        self.fee_growth_global0_x128, 
                        self.fee_growth_global1_x128
                    )?
                }, 
                None => {
                    (U256::ZERO, U256::ZERO)
                }
            }; 
            
            let _fee_inside0 = mul_div(fee_growth_inside0_x128, U256::from(info.liquidity_gross), Q128)?; 
            let _fee_inside1 = mul_div(fee_growth_inside1_x128, U256::from(info.liquidity_gross), Q128)?;  
            println!("good");
            fee_inside0.push(_fee_inside0.to_string()); 
            fee_inside1.push(_fee_inside1.to_string()); 
        }

        let tick_series = Series::new("tick", tick); 
        let liquidity_net_series = Series::new("liquidity_net", liquidity_net); 
        let liquidity_gross_series = Series::new("liquidity_gross", liquidity_gross); 
        let fee_inside0_series = Series::new("fee_inside_0", fee_inside0);
        let fee_inside1_series = Series::new("fee_inside_1", fee_inside1); 

        let series_vector = vec![tick_series, liquidity_net_series, liquidity_gross_series, fee_inside0_series, fee_inside1_series]; 

        let mut df = DataFrame::new(series_vector)?; 
        let mut file = File::create("example.csv").expect("could not create file");
        CsvWriter::new(&mut file).include_header(true).with_separator(b',').finish(&mut df)?; 

        println!("{:?}", df); 

        Ok(df)
    }
}

pub async fn get_pool_address(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    pair: (Address, Address),
    fee: u32
) -> Result<Address> {

    let pool_factory = IPoolFactory::new(pool_factory_address, provider);

    match pool_factory.getPool(pair.0, pair.1, fee).call().await? {
        IPoolFactory::getPoolReturn {pool} => if pool != Address::ZERO {Ok(pool)} else {Err(eyre!("Pool not found for pair: {:?} and fee: {}", pair, fee))},
    }
}

pub async fn simulate_exact_input_single(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    pair: (Address, Address), 
    amount_in: U256,
    one_for_two: bool
) -> Result<SwapResult> {

    let mut pool_state = PoolState::load(provider, pool_factory_address, pair, 10000, LoadingPattern::MID).await?; 

    let zero_for_one = if pair.0 == pool_state.token0.address {one_for_two} else {!one_for_two}; 
    let (amount0, amount1) = swap::swap(
        provider,
        &mut pool_state,
        zero_for_one, 
        math::safe_cast::to_int256(amount_in)?, 
        if zero_for_one {MIN_SQRT_RATIO + U256::from(1)} else {MAX_SQRT_RATIO - U256::from(1)}
    ).await?;

    let amount_out = (if zero_for_one {amount1} else {amount0}).unsigned_abs();

    Ok(SwapResult{amount_in, amount_out})
}

pub async fn simulate_swap_slippage(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    pair: (Address, Address),
    one_for_two: bool, 
    price_impact: u32
) -> Result<SwapResultSlippage> {

    let mut pool_state = PoolState::load(provider, pool_factory_address, pair, 10000, LoadingPattern::MID).await?; 

    let zero_for_one = if pair.0 == pool_state.token0.address {one_for_two} else {!one_for_two}; 
    let ((amount0, amount1), state_exec_sqrt_price_x96) = swap::swap_slippage(
        provider,
        &mut pool_state,
        zero_for_one,
        price_impact
    ).await?;

    let mut exec_sqrt_price_x96 = full_math::mul_div(
        sqrt(U256::from((-amount1).into_raw()))?, 
        Q96, 
        sqrt(U256::from(amount0.into_raw()))?
    )?;

    exec_sqrt_price_x96 = state_exec_sqrt_price_x96;

    //println!("{}, {}", state_exec_sqrt_price_x96, exec_sqrt_price_x96);

    let exec_price_impact = U256::from(100000) - if zero_for_one {
        full_math::mul_div(exec_sqrt_price_x96, U256::from(1000), pool_state.slot0.sqrt_price_x96)?
    } else {
        full_math::mul_div(pool_state.slot0.sqrt_price_x96, U256::from(1000), exec_sqrt_price_x96)?
    }.pow(U256_2) / U256::from(10);

    let amount_out = (if zero_for_one {amount1} else {amount0}).unsigned_abs();
    let amount_in = (if zero_for_one {amount0} else {amount1}).unsigned_abs();

    Ok(SwapResultSlippage{amount_in, amount_out, price_impact: exec_price_impact})
}

#[cfg(test)]
mod tests {
    use alloy::{
        primitives::{address, U256}, providers::ProviderBuilder}; 
    use crate::uniswap_v3::{utils::UNISWAP_V3_POOL_FACTORY_ADDRESS, quoter};
    use super::*; 

    #[tokio::test]
    async fn simulate_exact_input_single_test() {
        let rpc_url = "https://eth.llamarpc.com".parse().unwrap();
        // Create a provider with the HTTP transport using the `reqwest` crate.
        let provider = ProviderBuilder::new().on_http(rpc_url);

        let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
        let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

        let amount_in = U256::from(20000000000000000 as u128); 

        assert_eq!(
            simulate_exact_input_single(&provider, UNISWAP_V3_POOL_FACTORY_ADDRESS, (weth, usdc), amount_in, false).await.unwrap(), 
            quoter::_quote_exact_input_single(&provider, (weth, usdc), amount_in, false).await.unwrap()
        );  
    }

    #[tokio::test]
    async fn simulate_swap_slippage_test() {
        let rpc_url = "https://eth.llamarpc.com".parse().unwrap();
        // Create a provider with the HTTP transport using the `reqwest` crate.
        let provider = ProviderBuilder::new().on_http(rpc_url);

        let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
        let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

        let mut price_impact = 10; 

        let mut swap_result = simulate_swap_slippage(&provider, UNISWAP_V3_POOL_FACTORY_ADDRESS, (weth, usdc), true, price_impact).await.unwrap(); 
        println!("Swap Result : {:?}", swap_result); 
        //assert less than 1% difference between executed price impact and initial price impact
        assert!(swap_result.price_impact - U256::from(price_impact * 1000) < U256::from(1000));  

        price_impact = 20;
        swap_result = simulate_swap_slippage(&provider, UNISWAP_V3_POOL_FACTORY_ADDRESS, (weth, usdc), true, price_impact).await.unwrap(); 
        println!("Swap Result : {:?}", swap_result); 
        //assert less than 1% difference between executed price impact and initial price impact
        assert!(swap_result.price_impact - U256::from(price_impact * 1000) < U256::from(1000));  
    }

}