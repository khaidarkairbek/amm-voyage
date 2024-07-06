use alloy::{
    primitives::{address, U256}, providers::ProviderBuilder};
mod uniswap_v3;  

#[tokio::main]
async fn main() -> Result<(), String>{
    // Set up the HTTP transport which is consumed by the RPC client.
    let rpc_url = "https://eth.llamarpc.com".parse().unwrap();
    // Create a provider with the HTTP transport using the `reqwest` crate.
    let provider = ProviderBuilder::new().on_http(rpc_url);

    let pool_factory_address = address!("1F98431c8aD98523631AE4a59f267346ea31F984");

    let weth = address!("C02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2");
    let usdc = address!("A0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48");
    uniswap_v3::load_pool_state(&provider, pool_factory_address, (weth, usdc), 10000).await?;
    println!("Amount out: {:?}", uniswap_v3::simulate_exact_input_single(&provider, pool_factory_address, (weth, usdc), U256::from(200000000000 as u128), false).await.unwrap());
    //uniswap_v3::get_quote(&provider, usdc, weth).await;
    Ok(())
}