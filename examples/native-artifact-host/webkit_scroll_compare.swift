// Diagnostic only: trusted self-contained fixtures; NO production isolation API.
// Driver invokes --html PATH. RNDR_WK_COMPARE=iframe selects historical wrapper.
import AppKit
import WebKit
let app = NSApplication.shared
app.setActivationPolicy(.regular)
let config = WKWebViewConfiguration()
config.websiteDataStore = .nonPersistent()
let web = WKWebView(frame: NSRect(x:0,y:0,width:1200,height:778),configuration:config)
let window = NSWindow(contentRect:NSRect(x:40,y:60,width:1200,height:778), styleMask:[.titled,.closable,.resizable],backing:.buffered,defer:false)
window.title = "Owned standalone WK diagnostic"
window.contentView = web
class Delegate: NSObject, WKNavigationDelegate {
 func webView(_ webView: WKWebView,didFinish navigation: WKNavigation!) { fputs("host artifact: Ready\n",stdout);fflush(stdout) }
}
let delegate = Delegate(); web.navigationDelegate = delegate
var html = try String(contentsOfFile:CommandLine.arguments[2], encoding:.utf8)
if ProcessInfo.processInfo.environment["RNDR_WK_COMPARE"] == "iframe" {
    let escaped=(html + "\n<style>html:root{overscroll-behavior:none!important}</style>")
        .replacingOccurrences(of:"&",with:"&amp;").replacingOccurrences(of:"<",with:"&lt;")
        .replacingOccurrences(of:">",with:"&gt;").replacingOccurrences(of:"\"",with:"&quot;")
        .replacingOccurrences(of:"'",with:"&#39;")
    html="""
    <!doctype html><html><head><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; frame-src 'self'; connect-src 'none'; font-src 'none'; media-src 'none'; object-src 'none'; worker-src 'none'; base-uri 'none'; form-action 'none'"><style>html,body{margin:0;width:100%;height:100%;overflow:hidden}iframe{border:0;width:100%;height:100%;display:block}</style></head><body><iframe sandbox="allow-scripts" referrerpolicy="no-referrer" allow="camera 'none'; microphone 'none'; geolocation 'none'; clipboard-read 'none'; clipboard-write 'none'; display-capture 'none'; fullscreen 'none'; payment 'none'; usb 'none'" srcdoc="\(escaped)"></iframe></body></html>
    """
}
web.loadHTMLString(html,baseURL:nil)
window.makeKeyAndOrderFront(nil); app.activate(ignoringOtherApps:true)
app.run()
