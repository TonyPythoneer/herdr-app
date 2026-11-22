//! Native trusted event proof: real GPUI child + WKWebView local fixture.
#![cfg_attr(not(target_os="macos"), allow(dead_code,unused_imports))]

#[cfg(target_os="macos")]
#[path="../src/wk_input.rs"]
mod wk_input;

#[cfg(target_os="macos")]
mod mac {
    use gpui::{prelude::*,*};
    use raw_window_handle::HasWindowHandle;
    use std::{io::{Read,Write}, net::TcpListener, sync::mpsc, time::Duration};
    use super::wk_input;
    use wry::WebViewExtMacOS;

    struct Scene { web: Entity<gpui_wry::WebView> }
    impl Render for Scene {
        fn render(&mut self,_: &mut Window,_: &mut Context<Self>) -> impl IntoElement {
            div().size_full().child(self.web.clone())
        }
    }

    fn fixture() -> (String,std::thread::JoinHandle<()>) {
        let listener=TcpListener::bind("127.0.0.1:0").unwrap();
        let url=format!("http://{}/",listener.local_addr().unwrap());
        let handle=std::thread::spawn(move ||{
            let (mut conn,_) = listener.accept().unwrap();
            conn.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut buf=[0u8;4096];
            let n=conn.read(&mut buf).unwrap_or(0);
            assert!(String::from_utf8_lossy(&buf[..n]).starts_with("GET / HTTP/1.1"));
            let html=r#"<!doctype html><meta name="viewport" content="width=device-width,initial-scale=1">
<body style="margin:0;background:white">
<button id="trusted" style="position:absolute;left:20px;top:20px;width:120px;height:60px">Tap</button>
<div id="status" style="position:absolute;top:120px">pending</div>
<script>
window.__pressed=false;window.__trusted=false;
document.getElementById('trusted').addEventListener('click',(e)=>{
 window.__pressed=true;window.__trusted=e.isTrusted;
 document.getElementById('status').textContent=e.isTrusted?'trusted':'synthetic';
});
</script></body>"#;
            write!(conn,"HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n{}",html.len(),html).unwrap();
            conn.flush().unwrap();
        });
        (url,handle)
    }

    async fn evaluate(web: &Entity<gpui_wry::WebView>,code: &'static str,cx: &mut AsyncApp) -> Result<String,String>{
        let (tx,rx)=mpsc::channel();
        cx.update(|cx|web.update(cx,|view,_|{
            view.raw().evaluate_script_with_callback(code,move |response|{let _=tx.send(response);})
        })).map_err(|e|format!("failed to invoke WebKit script: {e}"))?;
        cx.background_executor().spawn(async move{
            rx.recv_timeout(Duration::from_millis(800)).map_err(|e|format!("WebKit callback timeout: {e}"))
        }).await
    }

    async fn check(web: Entity<gpui_wry::WebView>,window: WindowHandle<Scene>,cx:&mut AsyncApp)->Result<(),String>{
        cx.update_window(window.into(), |_,win,cx|{let _=win.draw(cx);})
            .map_err(|e|format!("native GPUI frame unavailable: {e}"))?;
        let mut loaded=false;
        for _ in 0..30 {
            if evaluate(&web,"document.readyState==='complete' && !!document.getElementById('trusted')",cx).await.ok().as_deref()==Some("true"){
                loaded=true;break;
            }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
        if !loaded{return Err("WK input fixture did not load".into());}
        // The macOS runner may show a window without giving it key focus.
        // Make the actual visible AppKit window key before sending native input.
        cx.update(|cx|web.update(cx,|view,_| {
            if let Some(window)=view.raw().webview().window() {
                window.makeKeyAndOrderFront(None);
            }
        }));
        cx.background_executor().timer(Duration::from_millis(350)).await;

        cx.update(|cx|web.update(cx,|v,_|wk_input::click_at(v,80.0,50.0)))
            .map_err(|e|format!("native AppKit pointer was rejected: {e}"))?;
        let mut last=String::new();
        for _ in 0..35 {
            last=evaluate(&web,"window.__pressed===true && window.__trusted===true",cx).await.unwrap_or_default();
            if last=="true" {
                println!("WKWebView received trusted native AppKit click: {last}");
                return Ok(());
            }
            cx.background_executor().timer(Duration::from_millis(100)).await;
        }
        Err(format!("WKWebView did not receive a trusted native click; last observation: {last}"))
    }

    pub fn run(){
        let (url,server)=fixture();
        // The test must be fail-closed. Some AppKit run() implementations
        // exit the process when cx.quit() is called, skipping any assertions
        // after run() and producing false-green CI results.
        gpui_platform::application().run(move|cx|{
            let mut web_slot=None;
            let window=cx.open_window(WindowOptions{
                window_bounds:Some(WindowBounds::Windowed(Bounds::new(
                    point(px(70.),px(70.)),size(px(360.),px(280.))))),
                show:true,focus:true,titlebar:None,..Default::default()
            },|win,cx|{
                let parent=win.window_handle().expect("native GPUI handle");
                let native=wry::WebViewBuilder::new().with_url(&url)
                    .build_as_child(&parent).expect("Wry WKWebView child");
                let web=cx.new(|cx|gpui_wry::WebView::new(native,win,cx));
                web_slot=Some(web.clone());
                cx.new(|_|Scene{web})
            }).expect("native GPUI test window");
            let web=web_slot.unwrap();
            cx.activate(true);
            cx.spawn(async move |cx|{
                let outcome=check(web,window,cx).await;
                match outcome {
                    Ok(()) => {
                        eprintln!("WK_TRUSTED_INPUT_VERIFIED: native click had isTrusted=true");
                        std::process::exit(0);
                    }
                    Err(error) => {
                        eprintln!("WK_TRUSTED_INPUT_FAILED: {error}");
                        std::process::exit(1);
                    }
                }
            }).detach();
        });
        let _ = server;
        panic!("WK_TRUSTED_INPUT_FAILED: AppKit exited before proof completed");
    }
}

fn main(){
    #[cfg(target_os="macos")]
    if std::env::var_os("HERDR_WK_NATIVE_INPUT_E2E").is_some() { mac::run(); }
}
