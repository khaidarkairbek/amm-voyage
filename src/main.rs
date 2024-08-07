use alloy::{
    primitives::{address, U256}, providers::ProviderBuilder};
mod uniswap_v3;  
use eyre::{eyre, Result};
use uniswap_v3::utils::UNISWAP_V3_POOL_FACTORY_ADDRESS;

#[tokio::main]
async fn main() -> Result<()>{
    // Set up the HTTP transport which is consumed by the RPC client.
    let rpc_url = "https://eth.llamarpc.com".parse().unwrap();
    // Create a provider with the HTTP transport using the `reqwest` crate.
    let provider = ProviderBuilder::new().on_http(rpc_url);

    let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
    let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
    println!("Amount out: {:?}", uniswap_v3::pool::simulate_exact_input_single(&provider, UNISWAP_V3_POOL_FACTORY_ADDRESS, (weth, usdc), U256::from(20000000000000000 as u128), false).await.unwrap());
    println!("Amount out: {:?}", uniswap_v3::quoter::_quote_exact_input_single(&provider, (weth, usdc), U256::from(20000000000000000 as u128), false).await.unwrap());
    Ok(())
}