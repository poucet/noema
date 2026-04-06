/// Result of dispatching an RPC method.
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
