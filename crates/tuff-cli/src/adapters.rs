pub fn mcp_register_tool(
    _repo_root: &std::path::Path,
    mcp_config_path: &std::path::Path,
    tool_id: &str,
    command: &str,
    args: &[String],
) -> tuff_core::error::Result<()> {
    tuff_core::mcp::register_tool(mcp_config_path, tool_id, command, args)
}

pub fn mcp_remove_tool(
    _repo_root: &std::path::Path,
    mcp_config_path: &std::path::Path,
    tool_id: &str,
) -> tuff_core::error::Result<()> {
    tuff_core::mcp::remove_tool(mcp_config_path, tool_id)
}
