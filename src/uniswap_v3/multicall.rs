use alloy::{
    sol, 
    providers::RootProvider, 
    transports::http::{Client, Http}, 
    primitives::{Address, address}
}; 
use eyre::Result;

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
) -> Result<Vec<IMulticall3::Result>>{
    let multicall_address = address!("cA11bde05977b3631167028862bE2a173976CA11"); 


    let call = call_data_list
    .into_iter()
    .map(|call_data| {
        IMulticall3::Call3{target: address, allowFailure: allow_failure, callData: call_data.into()}
    })
    .collect();

    let multicall = IMulticall3::new(multicall_address, provider); 
    match multicall.aggregate3(call).call().await? {
        IMulticall3::aggregate3Return{returnData} => Ok(returnData),
    }
}