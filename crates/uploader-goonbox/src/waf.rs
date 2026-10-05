//! Solve GoonBox's Cloudflare Turnstile widget with a real Chrome.
//!
//! GoonBox puts a Turnstile widget on `/login` and its login endpoint verifies
//! the resulting token server-side, so no HTTP-only client can authenticate.
//! The widget runs in managed mode and solves itself, but Chrome still has to
//! be a plausible browser for Cloudflare to accept the token.
//!
//! Ported from `manga-scraper-rs`'s `src/waf.rs`. Two details carry the whole
//! thing and are easy to get wrong:
//!
//! * The widget lives inside a closed shadow root, so the driver frame list
//!   cannot see it. It has to be found through `DOM.getDocument
//!   { pierce: true }`, which walks shadow roots and content documents.
//! * Chrome rejects automation unless the driver announces itself honestly, so
//!   `apply_native_profile` plus a persistent profile directory are both
//!   required, not optional hardening.

use anyhow::{anyhow, bail, Context, Result};
use chaser_oxide::cdp::browser_protocol::{
    dom::{GetBoxModelParams, GetDocumentParams, ScrollIntoViewIfNeededParams},
    page::AddScriptToEvaluateOnNewDocumentParams,
    target::CreateTargetParams,
};
use chaser_oxide::layout::Point;
use chaser_oxide::{Browser, BrowserConfig, ChaserPage, Page};
use futures::StreamExt;
use rand::Rng;
use std::path::Path;
use std::time::{Duration, Instant};

/// Let the widget solve itself for this long before any clicking starts.
/// Managed-mode Turnstile usually self-solves, and clicking during the
/// automatic pass is what makes it report a verification failure.
const PASSIVE_WAIT: Duration = Duration::from_secs(8);

/// Minimum gap between click attempts.
const CLICK_INTERVAL: Duration = Duration::from_millis(1_500);

/// How long to keep polling for the token after a click lands.
const POST_CLICK_WAIT: Duration = Duration::from_secs(20);

/// Give up after this many clicks. Retrying a challenge Cloudflare has already
/// rejected only raises the chance of a rate limit, and a fresh challenge is
/// wanted when it happens, not fifty more clicks on the same one.
const MAX_ATTEMPTS: u32 = 3;

/// Overall budget for the whole solve.
const SOLVE_TIMEOUT: Duration = Duration::from_secs(120);

/// The Turnstile checkbox sits at a fixed offset from the widget's top-left
/// corner; the widget's centre is a fallback for variants that ignore it.
const CHECKBOX_OFFSET: f64 = 30.0;

const READ_TOKEN: &str = r#"
(() => {
  const input = document.querySelector('input[name="cf-turnstile-response"]');
  return input && input.value ? input.value : null;
})()
"#;

/// Load the login page and return the Turnstile token it produces.
pub async fn solve(login_url: &str, profile_dir: &Path) -> Result<String> {
    let _display = VirtualDisplay::start_if_needed();
    let profile = resolve_profile(profile_dir)?;

    let config = BrowserConfig::builder()
        .viewport(None)
        .with_head()
        .hide()
        .user_data_dir(&profile)
        .build()
        .map_err(|e| anyhow!("browser config: {e}"))?;

    let (browser, mut handler) = Browser::launch(config)
        .await
        .context("failed to launch Chrome")?;
    tokio::spawn(async move { while handler.next().await.is_some() {} });

    let page = browser
        .new_page(CreateTargetParams::new("about:blank"))
        .await
        .context("failed to create page")?;

    let chaser = ChaserPage::new(page.clone());
    chaser
        .apply_native_profile()
        .await
        .context("failed to apply native profile")?;

    page.execute(AddScriptToEvaluateOnNewDocumentParams {
        source: WEBDRIVER_OVERRIDE.to_string(),
        world_name: None,
        include_command_line_api: None,
        run_immediately: None,
    })
    .await
    .context("failed to inject webdriver override")?;

    chaser
        .goto(login_url)
        .await
        .context("failed to open the goonbox login page")?;

    match wait_for_token(&chaser).await {
        Some(token) => Ok(token),
        None => {
            drop(browser);
            bail!(
                "the goonbox captcha did not clear within {}s. Chrome needs a display: on a \
                 headless server install Xvfb (apt install xvfb), or open Chrome once with the \
                 profile at {} and solve the challenge there.",
                SOLVE_TIMEOUT.as_secs(),
                profile.display()
            )
        }
    }
}

/// Poll for the widget's token, clicking only once it has stopped trying to
/// solve itself. `None` means the budget ran out.
async fn wait_for_token(chaser: &ChaserPage) -> Option<String> {
    let started = Instant::now();
    let mut last_click = started;
    let mut attempt = 0u32;

    while started.elapsed() < SOLVE_TIMEOUT {
        if let Some(token) = read_token(chaser).await {
            note(&format!(
                "widget solved itself after {:.1}s",
                started.elapsed().as_secs_f64()
            ));
            return Some(token);
        }

        if started.elapsed() >= PASSIVE_WAIT && last_click.elapsed() >= CLICK_INTERVAL {
            attempt += 1;
            match click_challenge(chaser, attempt).await {
                Some(target) => note(&format!(
                    "attempt {attempt}: clicked the widget at ({:.0},{:.0})",
                    target.0, target.1
                )),
                None => note(&format!("attempt {attempt}: the widget was not in the DOM")),
            }

            last_click = Instant::now();

            // The click starts a round trip to Cloudflare, so the token shows
            // up seconds later, not immediately. Sampling once right after the
            // click reads an empty input and gives up on a solve that is about
            // to succeed.
            if let Some(token) = wait_after_click(chaser).await {
                note("widget accepted the click");
                return Some(token);
            }
            note("no token after the click; the widget rejects it");

            if attempt >= MAX_ATTEMPTS {
                return None;
            }
        }

        tokio::time::sleep(Duration::from_millis(400)).await;
    }

    None
}

/// Progress goes to stderr so it does not pollute the uploader's stdout, which
/// callers parse for URLs.
fn note(message: &str) {
    eprintln!("goonbox captcha: {message}");
}

/// Poll long enough for Cloudflare to answer a click. Managed-mode Turnstile
/// shows a spinner for a few seconds before the token lands.
async fn wait_after_click(chaser: &ChaserPage) -> Option<String> {
    let deadline = Instant::now() + POST_CLICK_WAIT;

    loop {
        if let Some(token) = read_token(chaser).await {
            return Some(token);
        }
        if Instant::now() >= deadline {
            return None;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

async fn read_token(chaser: &ChaserPage) -> Option<String> {
    let value = chaser
        .evaluate(READ_TOKEN)
        .await
        .ok()
        .flatten()
        .and_then(|v| v.as_str().map(String::from))?;
    (!value.is_empty()).then_some(value)
}

/// One interaction attempt, returning the coordinates actually clicked so the
/// caller can report them. `None` means the widget could not be located.
async fn click_challenge(chaser: &ChaserPage, attempt: u32) -> Option<(f64, f64)> {
    let page = chaser.raw_page();

    let doc = page
        .execute(GetDocumentParams {
            depth: Some(-1),
            pierce: Some(true),
        })
        .await
        .ok()?;

    let node_id = find_widget_node(&doc.result.root)?;

    // Chrome ignores clicks aimed outside the viewport, so the widget has to be
    // on screen before its geometry means anything.
    let _ = page
        .execute(ScrollIntoViewIfNeededParams {
            node_id: Some(node_id),
            backend_node_id: None,
            object_id: None,
            rect: None,
        })
        .await;

    let box_model = page
        .execute(GetBoxModelParams {
            node_id: Some(node_id),
            backend_node_id: None,
            object_id: None,
        })
        .await
        .ok()?;

    let quad = box_model.result.model.content.inner();
    if quad.len() < 8 {
        return None;
    }

    let (left, top, right, bottom) = (quad[0], quad[1], quad[2], quad[5]);
    let target = match attempt % 3 {
        0 => (left + CHECKBOX_OFFSET, top + CHECKBOX_OFFSET),
        1 => ((left + right) / 2.0, (top + bottom) / 2.0),
        _ => (left + CHECKBOX_OFFSET - 5.0, top + CHECKBOX_OFFSET - 5.0),
    };

    move_and_click(page, target.0, target.1).await;

    // A keyboard gesture needs no geometry, so keep it in the rotation for
    // widgets that swallow synthetic mouse events.
    if attempt % 3 == 2 {
        move_and_click(page, (left + right) / 2.0, (top + bottom) / 2.0).await;
        let _ = chaser.press_key("Space").await;
    }

    Some(target)
}

fn find_widget_node(node: &DomNode) -> Option<NodeId> {
    if node.node_name == "IFRAME" && is_turnstile_iframe(node) {
        return Some(node.node_id);
    }

    for child in node.children.iter().flatten() {
        if let Some(id) = find_widget_node(child) {
            return Some(id);
        }
    }
    for shadow in node.shadow_roots.iter().flatten() {
        for child in shadow.children.iter().flatten() {
            if let Some(id) = find_widget_node(child) {
                return Some(id);
            }
        }
    }
    if let Some(doc) = &node.content_document {
        if let Some(id) = find_widget_node(doc) {
            return Some(id);
        }
    }

    find_legacy_shadow_widget(node)
}

use chaser_oxide::cdp::browser_protocol::dom::{Node as DomNode, NodeId};

fn is_turnstile_iframe(node: &DomNode) -> bool {
    // The marker can sit in either half of the pair: the iframe carries
    // `src="https://challenges.cloudflare.com/..."`, so the value matches while
    // the name is just "src".
    attribute_pairs(node).into_iter().any(|(name, value)| {
        format!("{name} {value}")
            .to_lowercase()
            .contains("cloudflare")
    }) || attribute_pairs(node).into_iter().any(|(name, value)| {
        format!("{name} {value}")
            .to_lowercase()
            .contains("turnstile")
    })
}

/// CDP hands attributes over as a flat `[name, value, name, value, ...]` list.
fn attribute_pairs(node: &DomNode) -> Vec<(&str, &str)> {
    let attrs = node.attributes.iter().flatten().map(String::as_str);
    attrs
        .collect::<Vec<_>>()
        .chunks(2)
        .filter(|pair| pair.len() == 2)
        .map(|pair| (pair[0], pair[1]))
        .collect()
}

/// Older Turnstile layouts hid the interactive element in the shadow root of a
/// host whose child was the `cf-turnstile-response` input.
fn find_legacy_shadow_widget(node: &DomNode) -> Option<NodeId> {
    let children = node.children.as_deref().unwrap_or(&[]);

    let hosts_response_input = children.iter().any(|child| {
        attribute_pairs(child)
            .into_iter()
            .any(|(name, value)| name == "name" && value == "cf-turnstile-response")
    });

    if hosts_response_input {
        if let Some(shadow) = node.shadow_roots.iter().flatten().next() {
            for child in shadow.children.iter().flatten() {
                let hidden = attribute_pairs(child)
                    .into_iter()
                    .any(|(name, value)| name == "style" && value.contains("display: none"));
                if !hidden {
                    return Some(child.node_id);
                }
            }
        }
    }

    for child in children {
        if let Some(id) = find_legacy_shadow_widget(child) {
            return Some(id);
        }
    }
    for shadow in node.shadow_roots.iter().flatten() {
        for child in shadow.children.iter().flatten() {
            if let Some(id) = find_legacy_shadow_widget(child) {
                return Some(id);
            }
        }
    }

    None
}

/// Approach the target along a jittered bezier curve, then click. A straight
/// constant-velocity move is one of the easiest automation tells there is.
/// A human-like approach path: where the pointer starts, the two control
/// points, and how long to wait at each step. Drawn up front because the
/// `Send` future cannot hold an RNG across an await.
struct Approach {
    start: (f64, f64),
    control: [(f64, f64); 2],
    target: (f64, f64),
    delays_ms: Vec<u64>,
    settle_ms: u64,
}

impl Approach {
    fn draw(target: (f64, f64)) -> Self {
        let mut rng = rand::rng();

        let start = (
            target.0 + rng.random_range(-150.0..=-40.0_f64),
            target.1 + rng.random_range(-90.0..=90.0_f64),
        );
        let control = [
            (
                start.0
                    + (target.0 - start.0) * rng.random_range(0.2..=0.5)
                    + rng.random_range(-25.0..=25.0),
                start.1
                    + (target.1 - start.1) * rng.random_range(0.1..=0.4)
                    + rng.random_range(-30.0..=30.0),
            ),
            (
                start.0
                    + (target.0 - start.0) * rng.random_range(0.5..=0.8)
                    + rng.random_range(-15.0..=15.0),
                start.1
                    + (target.1 - start.1) * rng.random_range(0.5..=0.9)
                    + rng.random_range(-15.0..=15.0),
            ),
        ];

        let steps = rng.random_range(12..=20_u8);
        let mut delays_ms = Vec::with_capacity(steps as usize);
        for step in 1..=steps {
            let t = f64::from(step) / f64::from(steps);
            // Ease in and out: the speed curve matters more than the path shape.
            let speed = (4.0 * t * (1.0 - t)).max(0.1);
            delays_ms.push((rng.random_range(8.0..=22.0_f64) / speed).min(70.0) as u64);
        }

        Self {
            start,
            control,
            target,
            delays_ms,
            settle_ms: rng.random_range(40..=110),
        }
    }

    fn point_at(&self, step: usize) -> (f64, f64) {
        let count = self.delays_ms.len().max(1);
        let t = step as f64 / count as f64;
        let u = 1.0 - t;

        (
            u * u * u * self.start.0
                + 3.0 * u * u * t * self.control[0].0
                + 3.0 * u * t * t * self.control[1].0
                + t * t * t * self.target.0,
            u * u * u * self.start.1
                + 3.0 * u * u * t * self.control[0].1
                + 3.0 * u * t * t * self.control[1].1
                + t * t * t * self.target.1,
        )
    }
}

async fn move_and_click(page: &Page, tx: f64, ty: f64) {
    let approach = Approach::draw((tx, ty));

    for (step, delay) in approach.delays_ms.iter().enumerate() {
        let (x, y) = approach.point_at(step + 1);
        let _ = page.move_mouse(Point::new(x, y)).await;
        tokio::time::sleep(Duration::from_millis(*delay)).await;
    }

    tokio::time::sleep(Duration::from_millis(approach.settle_ms)).await;
    let _ = page.click(Point::new(tx, ty)).await;
}

/// Page-level driver tells. `apply_native_profile` covers the CDP-level ones;
/// without both, Cloudflare returns a token that fails verification.
const WEBDRIVER_OVERRIDE: &str = r#"
(function() {
  try {
    var proto = Object.getPrototypeOf(navigator);
    if (proto) {
      try { delete proto.webdriver; } catch (e) {}
      Object.defineProperty(proto, 'webdriver', {
        get: function() { return false; },
        configurable: true,
        enumerable: true
      });
    }
  } catch (e) {
    try { delete navigator.webdriver; } catch (e2) {}
  }
  var props = Object.getOwnPropertyNames(window);
  for (var i = 0; i < props.length; i++) {
    if (/^cdc_|^\$cdc_|^__webdriver|^__selenium|^__driver|^\$chrome_/.test(props[i])) {
      try { delete window[props[i]]; } catch (e) {}
    }
  }
})();
"#;

/// Chrome needs a display even when nobody is watching, because headless mode
/// is itself a detection signal. On Linux without `DISPLAY`, Xvfb supplies one.
struct VirtualDisplay {
    #[allow(dead_code)]
    child: Option<std::process::Child>,
}

impl VirtualDisplay {
    #[cfg(target_os = "linux")]
    fn start_if_needed() -> Self {
        if !std::env::var("DISPLAY").unwrap_or_default().is_empty() {
            return Self { child: None };
        }

        // A leftover Xvfb from a crashed run is reusable and cheaper than a
        // second one racing for the same display number.
        let lock = Path::new("/tmp/.X99-lock");
        if lock.exists() {
            unsafe { std::env::set_var("DISPLAY", ":99") };
            return Self { child: None };
        }

        let spawned = std::process::Command::new("Xvfb")
            .args([
                ":99",
                "-screen",
                "0",
                "1920x1080x24",
                "-ac",
                "+extension",
                "GLX",
                "+render",
                "-noreset",
            ])
            .spawn();

        let child = match spawned {
            Ok(child) => child,
            Err(_) => return Self { child: None },
        };

        unsafe { std::env::set_var("DISPLAY", ":99") };
        for _ in 0..50 {
            if lock.exists() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Self { child: Some(child) }
    }

    #[cfg(not(target_os = "linux"))]
    fn start_if_needed() -> Self {
        Self { child: None }
    }
}

fn resolve_profile(profile_dir: &Path) -> Result<std::path::PathBuf> {
    let path = if profile_dir.is_absolute() {
        profile_dir.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_default()
            .join(profile_dir)
    };
    std::fs::create_dir_all(&path)
        .with_context(|| format!("could not create the Chrome profile at {}", path.display()))?;
    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}
