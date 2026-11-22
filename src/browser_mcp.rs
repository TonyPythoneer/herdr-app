//! Host-only glue: supply authoritative Herdr browser command validation and
//! the existing control socket to the external adapter's MCP implementation.
use serde_json::Value;
use herdr_webview_adapter::browser_mcp::{ToolArg,ToolDefinition};

pub fn serve_stdio() -> i32 {
    let commands = crate::commands::COMMANDS.iter()
        .filter(|command| command.name.starts_with("browser."))
        .map(|command| ToolDefinition {
            name: command.name.to_owned(),
            description: command.about.to_owned(),
            args: command.args.iter().map(|arg| ToolArg {
                name: arg.name.to_owned(), required: arg.required,
            }).collect(),
        }).collect::<Vec<_>>();
    herdr_webview_adapter::browser_mcp::serve_stdio(
        &commands, env!("CARGO_PKG_VERSION"), |name: &str, args: &Value| {
            let command = crate::commands::find(name)
                .filter(|command|command.name.starts_with("browser."))
                .ok_or("Unknown browser tool")?;
            crate::commands::validate_args(command, args)?;
            if let Some(required) = crate::commands::missing(command, args) {
                return Err(format!("missing required argument {required}"));
            }
            herdr_webview_adapter::browser_mcp::forward_socket(
                &crate::mods::app_dir().join("herdr-app.sock"), name, args)
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_catalog_does_not_grant_approval() {
        assert!(crate::commands::find("browser.approve").is_none());
        assert!(crate::commands::find("browser.click").is_some());
    }
}
