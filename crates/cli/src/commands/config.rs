use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use upio::preprocess::PreprocessConfig;
use upio::registry::UploaderId;
use upio_config::{Config, ConfigKey};

use crate::cli::ConfigAction;
use crate::style::{self, Screen, Tone};

/// Run the `config` command: get, set, or print the config path.
pub fn run(action: ConfigAction, config_path: Option<&Path>) -> Result<()> {
    match action {
        ConfigAction::Get { key, show_secrets } => get(config_path, key, show_secrets),
        ConfigAction::Set { key, value } => set(config_path, &key, &value),
        ConfigAction::Path => {
            println!("{}", path_of(config_path).display());
            Ok(())
        }
    }
}

fn get(config_path: Option<&Path>, key: Option<String>, show_secrets: bool) -> Result<()> {
    let config = load_config(config_path)?;

    let Some(key) = key else {
        if style::interactive() {
            print_summary(&config, config_path, show_secrets);
        } else {
            println!("{}", raw_toml(&config, show_secrets)?);
        }
        return Ok(());
    };

    let key = parse_key(&key)?;
    let value = config.get_value(&key);
    if style::interactive() {
        let (text, tone) = display_value(key, &value, show_secrets);
        println!(
            "  {} = {}",
            style::paint(key.as_str(), Tone::Muted),
            style::paint(text, tone)
        );
    } else {
        println!("{}", raw_value(key, &value, show_secrets));
    }
    Ok(())
}

fn set(config_path: Option<&Path>, key: &str, value: &str) -> Result<()> {
    let key = parse_key(key)?;
    let mut config = load_config(config_path)?;
    config.set_value(&key, value)?;
    save_config(&config, config_path)?;

    let cleared = value.is_empty() || value == "none";
    let (glyph, glyph_tone, shown, value_tone) = if cleared {
        ("[-]", Tone::Muted, "(cleared)".to_string(), Tone::Muted)
    } else {
        let (text, tone) = display_value(key, value, false);
        ("[V]", Tone::Good, text, tone)
    };

    println!(
        "  {} {} = {}",
        style::paint(glyph, glyph_tone),
        style::paint(key.as_str(), Tone::Plain),
        style::paint(shown, value_tone)
    );
    println!(
        "      {}",
        style::paint(
            format!("saved to {}", path_of(config_path).display()),
            Tone::Muted
        )
    );
    Ok(())
}

fn print_summary(config: &Config, config_path: Option<&Path>, show_secrets: bool) {
    let screen = style::screen();
    for line in summary_lines(config, &path_of(config_path), show_secrets, screen) {
        println!("{}", line.trim_end());
    }
}

fn summary_lines(config: &Config, path: &Path, show_secrets: bool, screen: Screen) -> Vec<String> {
    let mut lines = vec![header(path, screen), String::new()];

    let key_width = ConfigKey::ALL
        .iter()
        .map(|key| style::visible_width(&format!("    {}", key.as_str())))
        .max()
        .unwrap_or(0);

    let mut section = "";
    let defaults = Config::default();
    let mut set = 0;
    for &key in ConfigKey::ALL {
        let value = config.get_value(&key);
        if !value.is_empty() && value != defaults.get_value(&key) {
            set += 1;
        }

        let prefix = key.as_str().split('.').next().unwrap_or_default();
        if prefix != section {
            if !section.is_empty() {
                lines.push(String::new());
            }
            section = prefix;
        }

        lines.push(value_row(
            &format!("    {}", key.as_str()),
            key_width,
            display_value(key, &value, show_secrets),
            screen,
        ));
    }

    lines.extend(preprocess_lines(config, screen));

    lines.push(String::new());
    lines.push(format!("  {}", style::rule(screen.width.saturating_sub(4))));

    let mut footer = format!("{set} of {} config keys set", ConfigKey::ALL.len());
    if !show_secrets
        && ConfigKey::ALL
            .iter()
            .any(|key| key.is_secret() && !config.get_value(key).is_empty())
    {
        footer.push_str(", secrets masked");
    }
    lines.push(format!("  {}", style::paint(footer, Tone::Muted)));
    lines
}

fn preprocess_lines(config: &Config, screen: Screen) -> Vec<String> {
    let name_width = UploaderId::ALL
        .iter()
        .map(|id| style::visible_width(id.name()))
        .chain(std::iter::once(style::visible_width("global")))
        .max()
        .unwrap_or(0);
    let strategy_width = 16;

    let mut lines = vec![
        String::new(),
        format!("  {}", style::paint("preprocess", Tone::Strong)),
    ];

    push_preprocess(
        &mut lines,
        "global",
        &config.preprocess,
        false,
        name_width,
        strategy_width,
        screen,
    );

    for id in UploaderId::ALL {
        match config.preprocess_override(id.name()) {
            Some(preprocess) => push_preprocess(
                &mut lines,
                id.name(),
                preprocess,
                true,
                name_width,
                strategy_width,
                screen,
            ),
            None => lines.push(format!(
                "    {}{}",
                style::pad_right(&style::paint(id.name(), Tone::Muted), name_width + 2),
                style::paint("inherits global", Tone::Muted)
            )),
        }
    }

    lines
}

#[allow(clippy::too_many_arguments)]
fn push_preprocess(
    lines: &mut Vec<String>,
    label: &str,
    preprocess: &PreprocessConfig,
    overridden: bool,
    name_width: usize,
    strategy_width: usize,
    screen: Screen,
) {
    let (state, tone) = preprocess_state(preprocess);
    let state = if overridden {
        format!("override: {state}")
    } else {
        state
    };
    let room = screen.width.saturating_sub(6 + name_width).max(1);
    let state = style::truncate(&state, room);
    lines.push(format!(
        "    {}{}",
        style::pad_right(&style::paint(label, Tone::Plain), name_width + 2),
        style::paint(state, tone)
    ));

    let rule_tone = if preprocess.enabled {
        Tone::Plain
    } else {
        Tone::Muted
    };
    for rule in &preprocess.rules {
        let params = params_text(&rule.params);
        let room = screen.width.saturating_sub(6 + strategy_width).max(1);
        let params = style::truncate(&params, room);
        let mut line = format!(
            "      {}",
            style::pad_right(
                &style::paint(rule.strategy.def().id, rule_tone),
                strategy_width
            )
        );
        if !params.is_empty() {
            line.push_str(&style::paint(params, Tone::Muted));
        }
        lines.push(line);
    }
}

fn preprocess_state(preprocess: &PreprocessConfig) -> (String, Tone) {
    let count = preprocess.rules.len();
    match (preprocess.enabled, count) {
        (false, 0) => ("disabled".to_string(), Tone::Muted),
        (false, count) => (format!("disabled, {count} {}", plural(count)), Tone::Warn),
        (true, 0) => ("enabled, no rules".to_string(), Tone::Warn),
        (true, count) => (format!("enabled, {count} {}", plural(count)), Tone::Good),
    }
}

fn plural(count: usize) -> &'static str {
    if count == 1 {
        "rule"
    } else {
        "rules"
    }
}

fn params_text(params: &serde_json::Value) -> String {
    let Some(map) = params.as_object() else {
        return params.to_string();
    };
    map.iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn header(path: &Path, screen: Screen) -> String {
    let left = format!(
        "  {} {}",
        style::paint("upio", Tone::Strong),
        style::paint("config", Tone::Plain)
    );
    let room = screen
        .width
        .saturating_sub(style::visible_width(&left) + 2)
        .max(1);
    let path = style::paint(
        style::truncate_tail(&path.display().to_string(), room),
        Tone::Muted,
    );
    style::spread(left, path, screen.width)
}

fn value_row(label: &str, label_width: usize, value: (String, Tone), screen: Screen) -> String {
    let label_width = label_width.min(screen.width.saturating_sub(8)).max(1);
    let label = style::pad_right(&style::truncate(label, label_width), label_width + 2);
    let room = screen.width.saturating_sub(label_width + 2).max(1);
    let text = style::truncate(&value.0, room);
    format!("{}{}", label, style::paint(text, value.1))
}

fn display_value(key: ConfigKey, value: &str, show_secrets: bool) -> (String, Tone) {
    if value.is_empty() {
        let label = match key {
            ConfigKey::GlobalDisabledUploaders | ConfigKey::GlobalDisabledDownloaders => "(none)",
            _ => "(unset)",
        };
        return (label.to_string(), Tone::Muted);
    }
    if key.is_secret() && !show_secrets {
        return ("[REDACTED]".to_string(), Tone::Warn);
    }
    (value.to_string(), Tone::Plain)
}

fn raw_value(key: ConfigKey, value: &str, show_secrets: bool) -> String {
    if key.is_secret() && !show_secrets && !value.is_empty() {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

fn raw_toml(config: &Config, show_secrets: bool) -> Result<String> {
    if show_secrets {
        Ok(config.to_toml()?)
    } else {
        masked_toml(config)
    }
}

/// Serialized TOML with every credential replaced, so the output still parses.
fn masked_toml(config: &Config) -> Result<String> {
    let mut masked = config.clone();
    for endpoint in [
        masked.bunkr.as_mut(),
        masked.gofile.as_mut(),
        masked.fileditch.as_mut(),
        masked.filester.as_mut(),
        masked.goonbox.as_mut(),
        masked.pixeldrain.as_mut(),
    ]
    .into_iter()
    .flatten()
    {
        for field in [
            &mut endpoint.token,
            &mut endpoint.password,
            &mut endpoint.link_password,
        ] {
            if field.is_some() {
                *field = Some("[REDACTED]".to_string());
            }
        }
    }
    Ok(masked.to_toml()?)
}

fn parse_key(input: &str) -> Result<ConfigKey> {
    if let Ok(key) = input.parse::<ConfigKey>() {
        return Ok(key);
    }
    let suggestion = ConfigKey::ALL
        .iter()
        .copied()
        .map(|key| (distance(input, key.as_str()), key))
        .filter(|(distance, _)| *distance <= 3)
        .min_by_key(|(distance, _)| *distance)
        .map(|(_, key)| key);
    match suggestion {
        Some(key) => bail!(
            "unknown config key '{}', did you mean '{}'?",
            input,
            key.as_str()
        ),
        None => bail!("unknown config key '{}'", input),
    }
}

fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (index, left) in a.chars().enumerate() {
        let mut current = vec![index + 1];
        for (offset, right) in b.iter().enumerate() {
            let substitution = previous[offset] + usize::from(left != *right);
            current.push(
                substitution
                    .min(previous[offset + 1] + 1)
                    .min(current[offset] + 1),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

fn path_of(config_path: Option<&Path>) -> PathBuf {
    config_path
        .map(Path::to_path_buf)
        .unwrap_or_else(Config::path)
}

fn load_config(config_path: Option<&Path>) -> Result<Config> {
    let path = path_of(config_path);
    if path.is_file() {
        Ok(Config::from_file(&path)?)
    } else {
        Ok(Config::default())
    }
}

fn save_config(config: &Config, config_path: Option<&Path>) -> Result<()> {
    config.to_file(&path_of(config_path))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use upio::preprocess::{PreprocessRule, PreprocessStrategy};
    use upio_config::UploaderEndpointConfig;

    fn screen() -> Screen {
        Screen {
            width: 100,
            height: 40,
        }
    }

    fn with_preprocess() -> Config {
        Config {
            bunkr: Some(UploaderEndpointConfig {
                preprocess: Some(PreprocessConfig {
                    enabled: true,
                    rules: ["normalize_name", "split_video"]
                        .iter()
                        .map(|id| PreprocessRule::new(PreprocessStrategy::by_id(id).unwrap()))
                        .collect(),
                }),
                ..Default::default()
            }),
            ..Config::default()
        }
    }

    #[test]
    fn summary_masks_secrets_by_default() {
        let mut config = Config::default();
        config
            .set_value(&ConfigKey::PixeldrainToken, "pd-secret")
            .unwrap();
        let text =
            summary_lines(&config, Path::new("/tmp/config.toml"), false, screen()).join("\n");
        assert!(text.contains("pixeldrain.token"));
        assert!(text.contains("[REDACTED]"));
        assert!(!text.contains("pd-secret"));
        assert!(text.contains("secrets masked"));
    }

    #[test]
    fn summary_can_show_secrets() {
        let mut config = Config::default();
        config
            .set_value(&ConfigKey::PixeldrainToken, "pd-secret")
            .unwrap();
        let text = summary_lines(&config, Path::new("/tmp/config.toml"), true, screen()).join("\n");
        assert!(text.contains("pd-secret"));
        assert!(!text.contains("secrets masked"));
    }

    #[test]
    fn summary_marks_unset_values() {
        let text = summary_lines(
            &Config::default(),
            Path::new("config.toml"),
            false,
            screen(),
        )
        .join("\n");
        assert!(text.contains("(unset)"));
        assert!(text.contains("(none)"));
        assert!(text.contains(&format!("0 of {} config keys set", ConfigKey::ALL.len())));
    }

    #[test]
    fn summary_lists_preprocess_rules() {
        let text = summary_lines(
            &with_preprocess(),
            Path::new("config.toml"),
            false,
            screen(),
        )
        .join("\n");
        assert!(text.contains("preprocess"));
        assert!(text.contains("enabled, 2 rules"));
        assert!(text.contains("override: enabled, 2 rules"));
        assert!(text.contains("normalize_name"));
        assert!(text.contains("split_video"));
        assert!(text.contains("replacement="));
    }

    #[test]
    fn summary_marks_inherited_preprocess() {
        let config = Config {
            preprocess: PreprocessConfig {
                enabled: true,
                rules: vec![PreprocessRule::new(
                    PreprocessStrategy::by_id("wrap_zip").unwrap(),
                )],
            },
            ..Config::default()
        };
        let text = summary_lines(&config, Path::new("config.toml"), false, screen()).join("\n");
        assert!(text.contains("global"));
        assert!(text.contains("enabled, 1 rule"));
        assert!(text.contains("bunkr"));
        assert!(text.contains("inherits global"));
    }

    #[test]
    fn summary_marks_disabled_preprocess() {
        let text = summary_lines(
            &Config::default(),
            Path::new("config.toml"),
            false,
            screen(),
        )
        .join("\n");
        assert!(text.contains("preprocess"));
        assert!(text.contains("disabled"));
    }

    #[test]
    fn summary_fits_the_screen() {
        let narrow = Screen {
            width: 40,
            height: 24,
        };
        for line in summary_lines(
            &Config::default(),
            Path::new("/tmp/config.toml"),
            false,
            narrow,
        ) {
            assert!(style::visible_width(&line) <= narrow.width);
        }
    }

    #[test]
    fn masked_toml_hides_every_credential() {
        let mut config = Config::default();
        config
            .set_value(&ConfigKey::PixeldrainToken, "pd-secret")
            .unwrap();
        config
            .set_value(&ConfigKey::GoonboxPassword, "goon-secret")
            .unwrap();
        let toml = masked_toml(&config).unwrap();
        assert!(!toml.contains("pd-secret"));
        assert!(!toml.contains("goon-secret"));
        assert!(toml.contains("[REDACTED]"));
    }

    #[test]
    fn mistyped_keys_get_a_suggestion() {
        let error = parse_key("bunkr.tokn").unwrap_err().to_string();
        assert_eq!(
            error,
            "unknown config key 'bunkr.tokn', did you mean 'bunkr.token'?"
        );
        assert_eq!(parse_key("bunkr.token").unwrap(), ConfigKey::BunkrToken);
    }

    #[test]
    fn unrelated_keys_have_no_suggestion() {
        let error = parse_key("nonsense").unwrap_err().to_string();
        assert_eq!(error, "unknown config key 'nonsense'");
    }
}
