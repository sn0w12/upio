//! Shared Chrome setup for upio services that need a real browser.
//!
//! A headed, hidden Chrome with a persistent profile is what gets these sites
//! past their bot checks. Every service uses the same profile so logins and
//! clearance cookies are paid for once instead of once per service.
//!
//! On Linux, Xvfb supplies a display. On Windows, Chrome is started on a
//! separate desktop, which keeps its window off the interactive desktop and out
//! of the taskbar.
//!
//! Chrome locks its user data directory, so sessions in one process are
//! serialized. A second process using the same profile fails with Chrome's own
//! lock error rather than corrupting it.

use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context, Result};
use chaser_oxide::cdp::browser_protocol::page::AddScriptToEvaluateOnNewDocumentParams;
use chaser_oxide::cdp::browser_protocol::target::CreateTargetParams;
use chaser_oxide::{Browser, BrowserConfig, ChaserPage, Handler, Page};
use futures::StreamExt;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_ALL, HANDLE, WAIT_OBJECT_0};
#[cfg(windows)]
use windows_sys::Win32::System::StationsAndDesktops::{CloseDesktop, CreateDesktopW, HDESK};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    CreateProcessW, TerminateProcess, WaitForSingleObject, PROCESS_INFORMATION,
    STARTF_USESHOWWINDOW, STARTUPINFOW,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

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
    #[cfg(windows)]
    _hidden: Option<HiddenChrome>,
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

        #[cfg(windows)]
        let (browser, handler, hidden) = match launch_hidden(&profile).await {
            Ok((browser, handler, hidden)) => (browser, handler, Some(hidden)),
            Err(error) => {
                eprintln!(
                    "upio-browser: could not launch Chrome on a hidden desktop ({error}); \
                     falling back to a visible window"
                );
                let (browser, handler) = launch_headed(&profile).await?;
                (browser, handler, None)
            }
        };

        #[cfg(not(windows))]
        let (browser, handler) = launch_headed(&profile).await?;

        let (browser, chaser) = prepare_session(browser, handler).await?;

        Ok(Self {
            _browser: browser,
            #[cfg(windows)]
            _hidden: hidden,
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

async fn launch_headed(profile: &Path) -> Result<(Browser, Handler)> {
    let config = BrowserConfig::builder()
        .viewport(None)
        .with_head()
        .hide()
        .user_data_dir(profile)
        .build()
        .map_err(|error| anyhow!("browser config: {error}"))?;

    Browser::launch(config)
        .await
        .context("failed to launch Chrome")
}

async fn prepare_session(browser: Browser, mut handler: Handler) -> Result<(Browser, ChaserPage)> {
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

    Ok((browser, chaser))
}

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

#[cfg(windows)]
#[derive(Debug)]
struct HiddenChrome {
    process: isize,
    desktop: isize,
}

#[cfg(windows)]
impl HiddenChrome {
    fn has_exited(&self) -> bool {
        unsafe { WaitForSingleObject(self.process as HANDLE, 0) == WAIT_OBJECT_0 }
    }
}

#[cfg(windows)]
impl Drop for HiddenChrome {
    fn drop(&mut self) {
        unsafe {
            let process = self.process as HANDLE;
            if !process.is_null() {
                TerminateProcess(process, 1);
                // Keep the profile lock held until Chrome has actually exited,
                // otherwise a queued session can race the old process.
                WaitForSingleObject(process, 2_000);
                CloseHandle(process);
            }
            let desktop = self.desktop as HDESK;
            if !desktop.is_null() {
                CloseDesktop(desktop);
            }
        }
    }
}

#[cfg(windows)]
static HIDDEN_DESKTOP_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

#[cfg(windows)]
async fn launch_hidden(profile: &Path) -> Result<(Browser, Handler, HiddenChrome)> {
    let executable = chaser_oxide::detection::default_executable(Default::default())
        .map_err(|error| anyhow!("could not find Chrome: {error}"))?;
    let hidden = spawn_hidden(&executable, profile)?;
    let port = wait_for_port(profile, &hidden).await?;

    let mut last_error = None;
    for _ in 0..100 {
        match Browser::connect(format!("http://127.0.0.1:{port}")).await {
            Ok((browser, handler)) => return Ok((browser, handler, hidden)),
            Err(error) => {
                last_error = Some(error);
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
        }
    }

    Err(anyhow!(
        "Chrome started on the hidden desktop but did not accept a CDP connection: {}",
        last_error.expect("the loop runs at least once")
    ))
}

#[cfg(windows)]
fn spawn_hidden(executable: &Path, profile: &Path) -> Result<HiddenChrome> {
    let _ = std::fs::remove_file(profile.join("DevToolsActivePort"));

    let desktop_label = format!(
        "upio-hidden-{}-{}",
        std::process::id(),
        HIDDEN_DESKTOP_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    );
    let desktop_name = wide(&desktop_label);
    let desktop = unsafe {
        CreateDesktopW(
            desktop_name.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            GENERIC_ALL,
            std::ptr::null(),
        )
    };
    if desktop.is_null() {
        return Err(anyhow!(
            "could not create a hidden desktop: {}",
            std::io::Error::last_os_error()
        ));
    }

    let desktop_path = format!("winsta0\\{desktop_label}");
    let mut desktop_path = wide(&desktop_path);

    let executable_label = executable.to_string_lossy();
    let executable_wide = wide(&executable_label);
    let mut command_line = quote_arg(&executable_label);
    for arg in chrome_args(profile) {
        command_line.push(' ');
        command_line.push_str(&quote_arg(&arg));
    }
    let mut command_line = wide(&command_line);

    let mut startup: STARTUPINFOW = unsafe { std::mem::zeroed() };
    startup.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
    startup.lpDesktop = desktop_path.as_mut_ptr();
    startup.dwFlags = STARTF_USESHOWWINDOW;
    startup.wShowWindow = SW_HIDE as u16;

    let mut process_info: PROCESS_INFORMATION = unsafe { std::mem::zeroed() };
    let created = unsafe {
        CreateProcessW(
            executable_wide.as_ptr(),
            command_line.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            0,
            std::ptr::null(),
            std::ptr::null(),
            &startup,
            &mut process_info,
        )
    };

    if created == 0 {
        let error = std::io::Error::last_os_error();
        unsafe { CloseDesktop(desktop) };
        return Err(anyhow!(
            "could not launch Chrome on the hidden desktop: {error}"
        ));
    }

    unsafe { CloseHandle(process_info.hThread) };
    Ok(HiddenChrome {
        process: process_info.hProcess as isize,
        desktop: desktop as isize,
    })
}

#[cfg(windows)]
async fn wait_for_port(profile: &Path, hidden: &HiddenChrome) -> Result<u16> {
    let path = profile.join("DevToolsActivePort");

    for _ in 0..200 {
        if let Ok(contents) = std::fs::read_to_string(&path) {
            if let Some(port) = contents
                .lines()
                .next()
                .and_then(|line| line.trim().parse().ok())
            {
                return Ok(port);
            }
        }
        if hidden.has_exited() {
            return Err(anyhow!("Chrome exited before its debug port was ready"));
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    Err(anyhow!("Chrome did not open a debug port within 20s"))
}

/// Chrome's default arguments from `chaser-oxide`, plus the flags this crate
/// needs. Kept in sync with `BrowserConfig::launch` because the hidden-desktop
/// path spawns Chrome directly.
#[cfg(windows)]
fn chrome_args(profile: &Path) -> Vec<String> {
    vec![
        "--disable-background-networking".to_string(),
        "--disable-background-timer-throttling".to_string(),
        "--disable-backgrounding-occluded-windows".to_string(),
        "--disable-breakpad".to_string(),
        "--disable-client-side-phishing-detection".to_string(),
        "--disable-component-extensions-with-background-pages".to_string(),
        "--disable-default-apps".to_string(),
        "--disable-dev-shm-usage".to_string(),
        "--disable-features=TranslateUI".to_string(),
        "--disable-hang-monitor".to_string(),
        "--disable-ipc-flooding-protection".to_string(),
        "--disable-popup-blocking".to_string(),
        "--disable-prompt-on-repost".to_string(),
        "--disable-renderer-backgrounding".to_string(),
        "--disable-sync".to_string(),
        "--force-color-profile=srgb".to_string(),
        "--no-first-run".to_string(),
        "--password-store=basic".to_string(),
        "--use-mock-keychain".to_string(),
        "--remote-debugging-port=0".to_string(),
        "--disable-extensions".to_string(),
        format!("--user-data-dir={}", profile.display()),
        "--disable-blink-features=AutomationControlled".to_string(),
    ]
}

#[cfg(windows)]
fn quote_arg(value: &str) -> String {
    if value.contains(' ') || value.contains('\t') {
        format!("\"{value}\"")
    } else {
        value.to_string()
    }
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
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
