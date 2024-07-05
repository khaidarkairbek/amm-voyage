use alloy::{
    primitives::{address, Address, U256, I256}, providers::{ProviderBuilder, RootProvider}, sol, transports::http::{Client, Http}};
mod uniswap_v3;  

#[tokio::main]
async fn main() -> Result<(), String>{
    // ...
    // Set up the HTTP transport which is consumed by the RPC client.
    let rpc_url = "https://eth.llamarpc.com".parse().unwrap();
    // Create a provider with the HTTP transport using the `reqwest` crate.
    let provider = ProviderBuilder::new().on_http(rpc_url);

    //let pool_factory_address = address!("B5F00c2C5f8821155D8ed27E31932CFD9DB3C5D5");
    let pool_factory_address = address!("1F98431c8aD98523631AE4a59f267346ea31F984");

    let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
    let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");

    let pool_address = get_pool_address(&provider, pool_factory_address, weth, usdc).await.unwrap();
    // Print the block number.
    println!("Pool address is: {pool_address}");

    get_swap_amount_for_user_price_impact(&provider, pool_factory_address, (weth, usdc), 20).await.unwrap();
    Ok(())
}


async fn get_pool_address(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    token1: Address, 
    token2: Address
) -> Result<Address, String> {
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

    let pool_factory = IPoolFactory::new(pool_factory_address, provider);

    match pool_factory.getPool(token1, token2, 10000).call().await {
        Ok(IPoolFactory::getPoolReturn {pool}) => Ok(pool), 
        Err(e) => Err(e.to_string())
    }
}


pub struct Slot0 {
    sqrt_price_x96: U256,
    tick: i32,
    unlocked: bool
}

async fn get_swap_amount_for_user_price_impact(
    provider: &RootProvider<Http<Client>>, 
    pool_factory_address: Address, 
    pair: (Address, Address), 
    exec_price_impact: u8
) -> Result<(), String>{
    let pool_address = get_pool_address(provider, pool_factory_address, pair.0, pair.1).await?; 

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
        }
    };

    let pool = IPool::new(pool_address, provider); 
    let current_slot0: Slot0 = match pool.slot0().call().await {
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
    let liquidity: u128 = pool.liquidity().call().await.map_err(|e| e.to_string())?._0; 
    let fee: u32 = pool.fee().call().await.map_err(|e| e.to_string())?._0;

    let (amount0, amount1) = uniswap_v3::pool::swap(
        provider, 
        pool_address, 
        &current_slot0, 
        liquidity, 
        tick_spacing, 
        fee,
        true, 
        I256::from_raw(U256::from(200000000000 as u128)), 
        current_slot0.sqrt_price_x96 * U256::from(97) / U256::from(100)
    ).await?; 

    println!("USDC sold: {:?}, ETH bought: {:?}", amount0, amount1);

    Ok(())
}
