use alloy::{
    primitives::{Address, U256}, 
    providers::RootProvider, 
    sol, 
    transports::http::{Client, Http}
};
use eyre::Result; 
use super::utils::UNISWAP_V3_QUOTER_ADDRESS;
use super::pool::SwapResult;

pub async fn _quote_exact_input_single(
    provider: &RootProvider<Http<Client>>,
    pair: (Address, Address), 
    amount_in: U256,
    one_for_two: bool
) -> Result<SwapResult> {
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

    let quoter = IQuoter::new(UNISWAP_V3_QUOTER_ADDRESS, provider); 
    match quoter.quoteExactInputSingle(token_in, token_out, 10000, amount_in, U256::ZERO).call().await? {
        IQuoter::quoteExactInputSingleReturn{amountOut} => Ok(SwapResult{amount_in, amount_out: amountOut}),
    }
}