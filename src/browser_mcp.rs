//! Optional MCP stdio adapter for the same Herdr window control socket.
//! This process never starts a second Chrome instance. It exposes only the
//! documented browser commands and never exposes a native approval action.
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    time::Duration,
};

const MCP_PROTOCOL: &str = "2025-06-18";
const MAX_LINE: usize = 1024 * 1024;

fn rpc_result(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

fn value_type(name: &str) -> &'static str {
    match name {
        "target"|"condition" => "object",
        "x"|"y"|"delta_y"|"delta_x" => "number",
        "timeout_ms"|"wait_ms"|"max_nodes" => "integer",
        "focus" => "boolean",
        _ => "string"
    }
}

fn browser_tools() -> Value {
    let tools = crate::commands::COMMANDS.iter()
        .filter(|command| command.name.starts_with("browser."))
        .map(|command| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::<&str>::new();
            for arg in command.args {
                properties.insert(arg.name.to_owned(),json!({"type":value_type(arg.name)}));
                if arg.required { required.push(arg.name) }
            }
            json!({
                "name":command.name,
                "description":command.about,
                "inputSchema":{
                    "type":"object",
                    "properties":properties,
                    "required":required,
                    "additionalProperties":false
                }
            })
        })
        .collect::<Vec<_>>();
    json!({"tools":tools})
}

fn forward(name: &str, args: &Value) -> Result<Value, String> {
    let command = crate::commands::find(name)
        .filter(|command|command.name.starts_with("browser."))
        .ok_or("Unknown browser tool")?;
    crate::commands::validate_args(command, args)?;
    if let Some(required) = crate::commands::missing(command, args) {
        return Err(format!("missing required argument {required}"));
    }
    let mut socket = UnixStream::connect(crate::mods::app_dir().join("herdr-app.sock"))
        .map_err(|e|format!("Herdr must be open to use browser tools: {e}"))?;
    socket.set_read_timeout(Some(Duration::from_secs(10)))
        .map_err(|e|e.to_string())?;
    let request = json!({"do":name,"args":args});
    let data = serde_json::to_vec(&request).map_err(|e|e.to_string())?;
    if data.len() > MAX_LINE { return Err("browser input exceeds 1MiB".into()) }
    socket.write_all(&data).and_then(|_|socket.write_all(b"\n"))
        .map_err(|e|e.to_string())?;
    let mut reply = String::new();
    let read = BufReader::new(&socket).read_line(&mut reply).map_err(|e|e.to_string())?;
    if read==0 {return Err("Herdr control socket closed without a result".into())}
    if reply.len()>MAX_LINE {return Err("Herdr control response exceeds 1MiB".into())}
    serde_json::from_str(&reply).map_err(|e|format!("invalid Herdr response: {e}"))
}

fn tool_response(result: Result<Value, String>) -> Value {
    match result {
        Ok(result) => {
            let is_error = result.get("error").is_some()
                || result.get("status").and_then(Value::as_str)==Some("failed");
            let structured = if result.is_object() {result.clone()} else {json!({"value":result})};
            json!({
                "content":[{"type":"text","text":result.to_string()}],
                "structuredContent":structured,
                "isError":is_error
            })
        }
        Err(error) => json!({
            "content":[{"type":"text","text":error}],
            "isError":true
        })
    }
}

fn process(request: &Value) -> Option<Value> {
    let id = request.get("id")?.clone();
    let Some(method) = request.get("method").and_then(Value::as_str) else {
        return Some(rpc_error(id,-32600,"invalid JSON-RPC request: method must be a string"));
    };
    let params=&request["params"];
    let result=match method {
        "initialize" => json!({
            "protocolVersion":MCP_PROTOCOL,
            "capabilities":{"tools":{"listChanged":false}},
            "serverInfo":{
                "name":"herdr-webview",
                "title":"Herdr WebView",
                "version":env!("CARGO_PKG_VERSION")
            }
        }),
        "ping" => json!({}),
        "tools/list" => browser_tools(),
        "prompts/list" => json!({"prompts":[]}),
        "resources/list" => json!({"resources":[]}),
        "tools/call" => {
            let Some(name)=params.get("name").and_then(Value::as_str) else {
                return Some(rpc_error(id,-32602,"tools/call requires the tool name"));
            };
            if !name.starts_with("browser.") || crate::commands::find(name).is_none() {
                return Some(rpc_error(id,-32602,"unknown browser tool"));
            }
            let args = params.get("arguments").cloned().unwrap_or_else(||json!({}));
            if !args.is_object(){
                return Some(rpc_error(id,-32602,"tools/call arguments must be an object"));
            }
            tool_response(forward(name,&args))
        }
        _ => return Some(rpc_error(id,-32601,"method not supported")),
    };
    Some(rpc_result(id,result))
}

pub fn serve_stdio() -> i32 {
    let stdin=io::stdin();
    let stdout=io::stdout();
    let mut output=stdout.lock();
    for line in stdin.lock().lines() {
        let line=match line {
            Ok(line) => line,
            Err(error) => {eprintln!("browser MCP stdin failed: {error}");return 1}
        };
        if line.len() > MAX_LINE {
            eprintln!("browser MCP rejected oversized request");
            return 1;
        }
        let request=match serde_json::from_str::<Value>(&line) {
            Ok(request) => request,
            Err(error) => {
                let answer=rpc_error(Value::Null,-32700,&format!("parse error: {error}"));
                if writeln!(output,"{answer}").is_err(){return 1}
                continue;
            }
        };
        if let Some(response)=process(&request) {
            if writeln!(output,"{response}").is_err() || output.flush().is_err() {
                return 1;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mcp_tools_use_the_commands_catalog_and_typed_schema() {
        let list=browser_tools();
        let tools=list["tools"].as_array().unwrap();
        assert!(tools.iter().all(|t|t["name"].as_str().unwrap().starts_with("browser.")));
        let click=tools.iter().find(|t|t["name"]=="browser.click").unwrap();
        assert_eq!(click["inputSchema"]["properties"]["target"]["type"],"object");
        assert!(click["inputSchema"]["required"].as_array().unwrap().contains(&json!("pane")));
        assert!(click["inputSchema"]["required"].as_array().unwrap().contains(&json!("target")));
        assert!(tools.iter().all(|t|t["name"]!="browser.approve"));
    }

    #[test]
    fn initialize_and_tool_errors_follow_json_rpc_2() {
        let init=process(&json!({"jsonrpc":"2.0","id":9,"method":"initialize","params":{}})).unwrap();
        assert_eq!(init["result"]["protocolVersion"],MCP_PROTOCOL);
        let invalid=process(&json!({"jsonrpc":"2.0","id":"abc","method":"tools/call",
            "params":{"name":"browser.unsafe","arguments":{}}})).unwrap();
        assert_eq!(invalid["error"]["code"],-32602);
        assert!(process(&json!({"jsonrpc":"2.0","method":"notifications/initialized"})).is_none());
    }

    #[test]
    fn MCP_cannot_approve_actions_even_when_a_client_requests_it() {
        assert!(crate::commands::find("browser.approve").is_none());
        assert!(forward("browser.approve",&json!({})).is_err());
        assert!(forward("terminal.kill",&json!({})).is_err());
    }
}
