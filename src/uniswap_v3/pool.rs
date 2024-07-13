use alloy::{
    primitives::{Address, Bytes, U256}, 
    providers::RootProvider, 
    sol, 
    sol_types::SolCall, 
    transports::http::{Client, Http}
};

use super::math::{tick::Info, tick_math::{MAX_SQRT_RATIO, MAX_TICK, MAX_WORD_POS, MIN_SQRT_RATIO, MIN_TICK, MIN_WORD_POS}};
use std::collections::HashMap; 
use eyre::{eyre, Result}; 
use super::{multicall::multicall, swap, math};

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

pub struct Slot0 {
    pub sqrt_price_x96: U256,
    pub tick: i32,
    pub unlocked: bool
}

pub struct PoolState {
    pub pool_address: Address,
    pub tick_spacing: i32, 
    pub fee: u32, 
    pub token0: Address, 
    pub token1: Address, 
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

pub enum LoadingPattern {
    LOW, 
    HIGH, 
    MID
}

impl PoolState {
    pub async fn load (
        provider: &RootProvider<Http<Client>>,
        pool_factory_address: Address, 
        pair: (Address, Address),
        fee: u32
    ) -> Result<Self> {
        let pool_address = get_pool_address(provider, pool_factory_address, pair, fee).await?;
        println!("Pool address {}",pool_address);
    
        let (slot0, tick_spacing, liquidity, fee, token0, token1) = {
    
            let encoded_calls = vec![
                IPool::slot0Call{}.abi_encode(), 
                IPool::tickSpacingCall{}.abi_encode(), 
                IPool::liquidityCall{}.abi_encode(), 
                IPool::feeCall{}.abi_encode(), 
                IPool::token0Call{}.abi_encode(), 
                IPool::token1Call{}.abi_encode()]; 
    
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
                IPool::token1Call::abi_decode_returns(&encoded_return_data[5], true)?._0
            )
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
            LoadingPattern::MID
        ).await?;
    
        let tick_bitmap: HashMap<i16, U256> = Self::get_tick_bitmap(
            provider, 
            pool_address, 
            word_pos, 
            LoadingPattern::MID
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
            ticks
        })
    }

    pub async fn get_ticks (
        provider: &RootProvider<Http<Client>>,
        pool_address: Address ,
        tick: i32, 
        tick_spacing: i32, 
        load: LoadingPattern 
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

        self.ticks = Self::get_ticks(provider, self.pool_address, next_tick, self.tick_spacing, load).await?;
        Ok(())
    }

    pub async fn get_tick_bitmap (
        provider: &RootProvider<Http<Client>>,
        pool_address: Address ,
        word_pos: i16,
        load: LoadingPattern 
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

        self.tick_bitmap = Self::get_tick_bitmap(provider, self.pool_address, word_pos, load).await?;
        Ok(())
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

    let mut pool_state = PoolState::load(provider, pool_factory_address, pair, 10000).await?;

    let zero_for_one = if pair.0 == pool_state.token0 {one_for_two} else {!one_for_two}; 
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
        let zero_for_one = false; 

        assert_eq!(
            simulate_exact_input_single(&provider, UNISWAP_V3_POOL_FACTORY_ADDRESS, (weth, usdc), amount_in, zero_for_one).await.unwrap(), 
            quoter::_quote_exact_input_single(&provider, (weth, usdc), amount_in, false).await.unwrap()
        );  
    }
}