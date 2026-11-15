//! Shared Chrome setup for upio services that need a real browser.
//!
//! A headed, hidden Chrome with a persistent profile is what gets these sites
//! past their bot checks. Every service uses the same profile so logins and
//! clearance cookies are paid for once instead of once per service.
//!
//! Chrome locks its user data directory, so sessions in one process are
//! serialized. A second process using the same profile fails with Chrome's own
//! lock error rather than corrupting it.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chaser_oxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chaser_oxide::cdp::browser_protocol::target::CreateTargetParams;
use chaser_oxide::{Browser, BrowserConfig, ChaserPage, Page};
use futures::StreamExt;

/// The one Chrome profile shared by every service.
pub fn profile_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("upio")
        .join("chrome-profile")
}

/// Create the shared profile directory and return its path.
pub fn profile_dir() -> std::io::Result<PathBuf> {
    let path = profile_path();
    std::fs::create_dir_all(&path)?;
    Ok(path)
}

/// One running Chrome instance with its first page prepared.
pub struct BrowserSession {
    _browser: Browser,
    _guard: tokio::sync::MutexGuard<'static, ()>,
    chaser: ChaserPage,
    profile: PathBuf,
}

impl BrowserSession {
    /// Launch Chrome with the shared profile.
    pub async fn launch() -> Result<Self> {
        let profile = profile_dir().context("could not create the shared Chrome profile")?;
        Self::launch_with_profile(&profile).await
    }

    /// Launch Chrome with an explicit profile.
    ///
    /// Used by callers that already had a configurable profile path before the
    /// shared profile existed.
    pub async fn launch_with_profile(profile: &Path) -> Result<Self> {
        let guard = PROFILE_LOCK.lock().await;
        let profile = prepare_profile(profile)?;
        ensure_display();

        let config = BrowserConfig::builder()
            .viewport(None)
            .with_head()
            .hide()
            .user_data_dir(&profile)
            .build()
            .map_err(|error| anyhow!("browser config: {error}"))?;

        let (browser, mut handler) = Browser::launch(config)
            .await
            .context("failed to launch Chrome")?;
        tokio::spawn(async move { while handler.next().await.is_some() {} });

        let page = browser
            .new_page(CreateTargetParams::new("about:blank"))
            .await
            .context("failed to create page")?;
        let chaser = ChaserPage::new(page);

        chaser
            .apply_native_profile()
            .await
            .context("failed to apply native profile")?;
        chaser
            .raw_page()
            .execute(AddScriptToEvaluateOnNewDocumentParams {
                source: WEBDRIVER_OVERRIDE.to_string(),
                world_name: None,
                include_command_line_api: None,
                run_immediately: None,
            })
            .await
            .context("failed to inject webdriver override")?;

        Ok(Self {
            _browser: browser,
            _guard: guard,
            chaser,
            profile,
        })
    }

    pub fn chaser(&self) -> &ChaserPage {
        &self.chaser
    }

    pub fn page(&self) -> &Page {
        self.chaser.raw_page()
    }

    pub fn profile(&self) -> &Path {
        &self.profile
    }

    pub async fn goto(&self, url: &str) -> Result<()> {
        self.chaser.goto(url).await
    }

    pub async fn evaluate(&self, script: &str) -> Result<Option<serde_json::Value>> {
        self.chaser.evaluate(script).await
    }
}

static PROFILE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn prepare_profile(profile: &Path) -> Result<PathBuf> {
    let path = if profile.is_absolute() {
        profile.to_path_buf()
    } else {
        std::env::current_dir()
            .context("could not resolve the Chrome profile path")?
            .join(profile)
    };
    std::fs::create_dir_all(&path)
        .with_context(|| format!("could not create the Chrome profile at {}", path.display()))?;
    Ok(std::fs::canonicalize(&path).unwrap_or(path))
}

/// Chrome needs a display even when the window is hidden. On Linux without
/// `DISPLAY`, Xvfb supplies one for the rest of the process.
#[cfg(target_os = "linux")]
fn ensure_display() {
    use std::sync::OnceLock;

    static DISPLAY: OnceLock<std::sync::Mutex<Option<std::process::Child>>> = OnceLock::new();
    DISPLAY.get_or_init(|| std::sync::Mutex::new(start_xvfb()));
}

#[cfg(not(target_os = "linux"))]
fn ensure_display() {}

#[cfg(target_os = "linux")]
fn start_xvfb() -> Option<std::process::Child> {
    if !std::env::var("DISPLAY").unwrap_or_default().is_empty() {
        return None;
    }

    let lock = Path::new("/tmp/.X99-lock");
    if lock.exists() {
        unsafe { std::env::set_var("DISPLAY", ":99") };
        return None;
    }

    let child = std::process::Command::new("Xvfb")
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
        .spawn()
        .ok()?;

    unsafe { std::env::set_var("DISPLAY", ":99") };
    for _ in 0..50 {
        if lock.exists() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    Some(child)
}

/// Page-level driver tells. `apply_native_profile` covers the CDP-level ones.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_profile_has_one_stable_name() {
        assert!(profile_path().ends_with(Path::new("upio").join("chrome-profile")));
    }

    #[test]
    fn prepare_profile_creates_an_absolute_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        let prepared = prepare_profile(&nested).unwrap();

        assert!(prepared.is_absolute());
        assert!(prepared.is_dir());
    }
}
