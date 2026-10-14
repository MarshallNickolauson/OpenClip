//! `clipcraft-mcp`: the stdio MCP server Claude launches. Speaks JSON-RPC on stdin/stdout.
fn main() {
    clipcraft_core::mcp::run_stdio();
}
