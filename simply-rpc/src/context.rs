/// Result of dispatching an RPC method.
///
/// Carries the RPC result alongside any streams produced by the method.
/// Each service defines its own stream type via `RpcService::Stream`.
pub struct DispatchResult<S = ()> {
    pub result: crate::RpcResult,
    pub streams: Vec<S>,
}

impl<S> DispatchResult<S> {
    pub fn value(result: crate::RpcResult) -> Self {
        Self {
            result,
            streams: Vec::new(),
        }
    }

    pub fn with_stream(result: crate::RpcResult, stream: S) -> Self {
        Self {
            result,
            streams: vec![stream],
        }
    }
}
