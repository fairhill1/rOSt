use super::types::{HttpState};
use super::http::parse_url;
use super::layout::load_html;
use alloc::string::{String, ToString};
use alloc::format;

/// Navigate to a URL (adds to history)
pub fn navigate(browser: &mut super::Browser, url: String) {
    // Add to history
    if browser.history_index < browser.history.len() {
        browser.history.truncate(browser.history_index);
    }
    browser.history.push(url.clone());
    browser.history_index = browser.history.len();

    // Load the page
    load_url(browser, url);
}

/// Load a URL without modifying history (used by back/forward)
pub fn load_url(browser: &mut super::Browser, url: String) {
    browser.url = url.clone();
    browser.url_input.set_text(&url);
    browser.scroll_offset = 0;
    browser.loading = true;

    // Handle special URLs
    if url.starts_with("about:") {
        load_about_page(browser, &url);
        browser.loading = false;
        return;
    }

    // Show loading page first
    load_html(browser, "<html><body><h1>Loading...</h1><p>Please wait while the page loads. This may take a few seconds.</p></body></html>".to_string());

    crate::kernel::uart_write_string(&format!("Browser: Async loading {}\r\n", url));

    // Parse URL to get host, port, path
    let (host, port, path) = parse_url(&url);

    crate::kernel::uart_write_string(&format!("Browser: Host={}, Port={}, Path={}\r\n", host, port, path));

    // Start async HTTP request - just initiate DNS resolution
    browser.http_state = HttpState::ResolvingDns {
        host,
        path,
        port,
        start_time: crate::kernel::drivers::timer::get_time_ms(),
    };

    // Returns immediately - poll_http() will advance the state machine
}

/// Go back in history
pub fn go_back(browser: &mut super::Browser) {
    if browser.history_index > 1 {
        browser.history_index -= 1;
        let url = browser.history[browser.history_index - 1].clone();

        // Load page without modifying history
        load_url(browser, url);
    }
}

/// Go forward in history
pub fn go_forward(browser: &mut super::Browser) {
    if browser.history_index < browser.history.len() {
        browser.history_index += 1;
        let url = browser.history[browser.history_index - 1].clone();

        // Load page without modifying history
        load_url(browser, url);
    }
}

/// Load error page
pub fn load_error_page(browser: &mut super::Browser, message: &str) {
    let html = format!(
        "<html><body><h1>Error</h1><p>{}</p></body></html>",
        message
    );
    load_html(browser, html);
}

/// Load about: page
pub fn load_about_page(browser: &mut super::Browser, url: &str) {
    let html = match url {
        "about:blank" => "<html><body></body></html>".to_string(),
        _ => format!(
            "<html><body>\
            <h1>rOSt Browser</h1>\
            <p>Version 1.0 - A simple web browser for rOSt</p>\
            <h2>Features</h2>\
            <ul>\
            <li>HTML parser with DOM tree</li>\
            <li>Text layout engine</li>\
            <li>Clickable hyperlinks</li>\
            <li>Address bar navigation</li>\
            <li>Keyboard shortcuts Ctrl+L</li>\
            </ul>\
            <h2>Current Limitations</h2>\
            <ul>\
            <li>Powered by smoltcp TCP/IP stack</li>\
            <li>No CSS support</li>\
            <li>Basic tags only h1-h6 p a ul ol li br div b i img table</li>\
            <li>BMP and PNG image support</li>\
            </ul>\
            <p>Use Terminal http command to test HTTP: <code>http example.com</code></p>\
            <p>Try clicking this test link <a href=\"about:blank\">about:blank</a></p>\
            </body></html>"
        ),
    };
    load_html(browser, html);
    browser.loading = false;
}
