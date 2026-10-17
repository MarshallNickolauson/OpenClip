//! `openclip-mcp`: the stdio MCP server Claude launches. Speaks JSON-RPC on stdin/stdout.
fn main() {
    openclip_core::mcp::run_stdio();
}
