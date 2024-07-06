pub mod pool;
pub mod math;

use alloy::{
    primitives::{address, Address, U256}, providers::RootProvider, sol, sol_types::SolCall, transports::http::{Client, Http}};
use math::tick_math::{MAX_SQRT_RATIO, MAX_WORD_POS, MIN_SQRT_RATIO, MIN_WORD_POS};
use std::collections::HashMap; 

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
    sqrt_price_x96: U256,
    tick: i32,
    unlocked: bool
}

pub struct PoolState {
    pool_address: Address,
    tick_spacing: i32, 
    fee: u32, 
    token0: Address, 
    token1: Address, 
    tick_bitmap: HashMap<i16, U256>, 
    slot0: Slot0, 
    liquidity: u128
}

#[derive(Debug)]
pub struct SwapResult {
    amount_in: U256, 
    amount_out: U256
}

pub async fn load_pool_state (
    provider: &RootProvider<Http<Client>>,
    pool_factory_address: Address, 
    pair: (Address, Address),
    fee: u32
) -> Result<PoolState, String> {
    let pool_address = get_pool_address(provider, pool_factory_address, pair, fee).await?;
    let pool = IPool::new(pool_address, provider);
    let slot0: Slot0 = match pool.slot0().call().await {
        Ok(IPool::slot0Return {
            sqrtPriceX96, 
            tick,
            unlocked,..}
        ) => {
            Slot0 {
                sqrt_price_x96: sqrtPriceX96,
                tick: tick,
                unlocked: unlocked
            }
        }, 
        Err(e) => return Err(e.to_string())
    };
    let tick_spacing = pool.tickSpacing().call().await.map_err(|e| e.to_string())?._0; 
    let liquidity = pool.liquidity().call().await.map_err(|e| e.to_string())?._0; 
    let fee: u32 = pool.fee().call().await.map_err(|e| e.to_string())?._0;
    let token0 = pool.token0().call().await.map_err(|e| e.to_string())?._0;
    let token1 = pool.token1().call().await.map_err(|e| e.to_string())?._0; 

    let tick_bitmap: HashMap<i16, U256> = {
        // Generate word position list for tick bitmap
        let word_pos_list: Vec<i16> = {
            let tick = slot0.tick; 
            let mut compressed: i32 = tick / tick_spacing;
            if tick < 0 && tick % tick_spacing != 0 {
                compressed = compressed - 1; 
            }
            let curr_tick_word_pos = (compressed >> 8) as i16; 
            (if MIN_WORD_POS > curr_tick_word_pos - 20 {MIN_WORD_POS} else {curr_tick_word_pos - 20} .. if MAX_WORD_POS < curr_tick_word_pos + 20 {MAX_WORD_POS} else {curr_tick_word_pos + 20}).collect()
        };


        let tick_bitmap_call_data: Vec<Vec<u8>> = word_pos_list
        .iter()
        .map(|&word_pos| {
            IPool::tickBitmapCall{wordPosition: word_pos}.abi_encode()
        })
        .collect(); 

        let return_data = multicall(provider, pool_address, false, tick_bitmap_call_data).await.unwrap();
        let mut result = Vec::<U256>::new();
        for data in return_data.iter() {
            let word = IPool::tickBitmapCall::abi_decode_returns(&data.returnData, true).map_err(|e| e.to_string())?._0; 
            result.push(word);
        }

        word_pos_list.into_iter().zip(result.into_iter()).collect()
    };

    Ok(PoolState{
        pool_address, 
        tick_spacing, 
        fee, 
        token0, 
        token1, 
        tick_bitmap, 
        slot0, 
        liquidity
    })
}

sol! {
    #[sol(rpc)]
    interface IMulticall3 {
        struct Call3 {
            // Target contract to call.
            address target;
            // If false, the entire call will revert if the call fails.
            bool allowFailure;
            // Data to call on the target contract.
            bytes callData;
        }
        
        struct Result {
            // True if the call succeeded, false otherwise.
            bool success;
            // Return data if the call succeeded, or revert data if the call reverted.
            bytes returnData;
        }
        
        /// @notice Aggregate calls, ensuring each returns success if required
        /// @param calls An array of Call3 structs
        /// @return returnData An array of Result structs
        function aggregate3(Call3[] calldata calls) public payable returns (Result[] memory returnData);
    }
}

pub async fn multicall (
    provider: &RootProvider<Http<Client>>,
    address: Address, 
    allow_failure: bool, 
    call_data_list: Vec<Vec<u8>>
) -> Result<Vec<IMulticall3::Result>, String>{
    let multicall_address = address!("cA11bde05977b3631167028862bE2a173976CA11"); 


    let call = call_data_list
    .into_iter()
    .map(|call_data| {
        IMulticall3::Call3{target: address, allowFailure: allow_failure, callData: call_data.into()}
    })
    .collect();

    let multicall = IMulticall3::new(multicall_address, provider); 
    match multicall.aggregate3(call).call().await {
        Ok(IMulticall3::aggregate3Return{returnData}) => Ok(returnData), 
        Err(e) => Err(e.to_string())
    }
}

pub async fn get_pool_address(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    pair: (Address, Address),
    fee: u32
) -> Result<Address, String> {

    let pool_factory = IPoolFactory::new(pool_factory_address, provider);

    match pool_factory.getPool(pair.0, pair.1, fee).call().await {
        Ok(IPoolFactory::getPoolReturn {pool}) => if pool != Address::ZERO {Ok(pool)} else {Err("Pool not found for given pair and fee".to_string())}, 
        Err(e) => Err(e.to_string())
    }
}

pub async fn simulate_exact_input_single(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    pair: (Address, Address), 
    amount_in: U256,
    one_for_two: bool
) -> Result<SwapResult, String>{

    let pool_state = load_pool_state(provider, pool_factory_address, pair, 10000).await?;

    let zero_for_one = if pair.0 == pool_state.token0 {one_for_two} else {!one_for_two}; 

    let (amount0, amount1) = pool::swap(
        provider,
        &pool_state,
        zero_for_one, 
        math::safe_cast::to_int256(amount_in)?, 
        if zero_for_one {MIN_SQRT_RATIO + U256::from(1)} else {MAX_SQRT_RATIO - U256::from(1)}
    ).await?;

    let amount_out = (if zero_for_one {amount1} else {amount0}).unsigned_abs();

    Ok(SwapResult{amount_in, amount_out})
}


pub async fn _quote_exact_input_single(
    provider: &RootProvider<Http<Client>>,
    pair: (Address, Address), 
    amount_in: U256,
    one_for_two: bool
) -> Result<SwapResult, String> {
    sol! {
        #[sol(rpc)]
        interface IQuoter {
            function quoteExactInputSingle(
                address tokenIn,
                address tokenOut,
                uint24 fee,
                uint256 amountIn,
                uint160 sqrtPriceLimitX96
            ) external returns (uint256 amountOut);
        }
    }

    let (token_in, token_out) = if one_for_two {pair } else {(pair.1, pair.0)}; 

    let quoter = IQuoter::new(address!("b27308f9F90D607463bb33eA1BeBb41C27CE5AB6"), provider); 
    match quoter.quoteExactInputSingle(token_in, token_out, 10000, amount_in, U256::ZERO).call().await {
        Ok(IQuoter::quoteExactInputSingleReturn{amountOut}) => Ok(SwapResult{amount_in, amount_out: amountOut}),
        Err(e) => Err(e.to_string())
    }
}