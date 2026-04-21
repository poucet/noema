// Public entry for @simply/client.
//
// Re-exports the transport layer and the codegen'd API clients + types.
// Consumers do:
//   import { getTransport, mcpApi, type McpServerInfo } from '@simply/client';

export * from './transport';
export * from './generated';
