pub mod config;

use {
    crate::config::Config,
    anyhow::{Context, Result},
    clap::Parser,
    env_logger::Env,
    figment::{
        Figment,
        providers::{Format, Serialized, Toml},
    },
    futures::stream::StreamExt,
    log::{debug, error, info},
    notify_rust::{Notification, NotificationHandle, Timeout},
    std::{path::PathBuf, process::Command, time::Duration},
    zbus::{Connection, proxy, zvariant::OwnedValue},
};

/// Simple program to send notifications on battery status changes
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to the configuration file
    #[arg(short, long)]
    config: Option<String>,
}

#[derive(Debug, OwnedValue)]
#[repr(u32)]
pub enum WarningLevel {
    Unknown = 0,
    None = 1,
    Discharging = 2,
    Low = 3,
    Critical = 4,
    Action = 5,
}

#[derive(Debug, OwnedValue)]
#[repr(u32)]
pub enum State {
    Unknown = 0,
    Charging = 1,
    Discharging = 2,
    Empty = 3,
    FullyCharged = 4,
    PendingCharge = 5,
    PendingDischarge = 6,
}

#[proxy(
    interface = "org.freedesktop.UPower.Device",
    default_service = "org.freedesktop.UPower",
    assume_defaults = false
)]
pub trait Device {
    #[zbus(property)]
    fn percentage(&self) -> zbus::Result<f64>;
    #[zbus(property)]
    fn time_to_empty(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn time_to_full(&self) -> zbus::Result<i64>;
    #[zbus(property)]
    fn warning_level(&self) -> zbus::Result<WarningLevel>;
    #[zbus(property)]
    fn state(&self) -> zbus::Result<State>;
    #[zbus(property)]
    fn online(&self) -> zbus::Result<bool>;
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let args = Args::parse();
    let config_path = if let Some(cfg) = args.config {
        PathBuf::from(cfg)
    } else {
        let xdg_dirs = xdg::BaseDirectories::with_prefix("upower-notify");
        xdg_dirs
            .get_config_file("config.toml")
            .context("failed to load XDG base directories")?
    };

    debug!("Looking for config at: {config_path:?}");
    let config: Config = Figment::from(Serialized::defaults(Config::default()))
        .merge(Toml::file(config_path))
        .extract()?;

    debug!("Config loaded: {config:#?}");
    info!("Using device {}", config.device);

    let connection = Connection::system().await?;
    let upower = DeviceProxy::new(&connection, config.device.as_str()).await?;
    let ac_power = DeviceProxy::new(&connection, "/org/freedesktop/UPower/devices/line_power_AC").await.ok();

    let mut warning_stream = upower.receive_warning_level_changed().await;
    let mut state_stream = upower.receive_state_changed().await;
    let mut percentage_stream = upower.receive_percentage_changed().await;
    let mut ac_online_stream = if let Some(ref ac) = ac_power {
        Some(ac.receive_online_changed().await)
    } else {
        None
    };

    let mut warning_notification: Option<NotificationHandle> = None;
    let mut state_notification: Option<NotificationHandle> = None;

    let parse_timeout = |t: u32| match t {
        0 => Timeout::Never,
        ms => Timeout::Milliseconds(ms),
    };

    // Seed from the actual current values before entering the loop. zbus's
    // receive_<property>_changed() streams emit the current cached value as
    // their first item on subscription, not just future PropertiesChanged
    // signals. Without this, that initial read is indistinguishable from a
    // real transition (None -> Some(current)) and fires a spurious
    // "just changed" notification on every service start (e.g. every niri
    // login), even when AC/percentage state hasn't actually changed.
    let mut last_pct: Option<u64> = upower.percentage().await.ok().map(|p| p.round() as u64);
    let mut last_ac_online: Option<bool> = if let Some(ref ac) = ac_power {
        ac.online().await.ok()
    } else {
        None
    };
    let mut last_ac_time: Option<std::time::Instant> = None;

    loop {
        tokio::select! {
            Some(msg) = async {
                if let Some(ref mut s) = ac_online_stream {
                    s.next().await
                } else {
                    futures::future::pending().await
                }
            } => {
                if let Ok(is_online) = msg.get().await {
                    if last_ac_online != Some(is_online) {
                        last_ac_online = Some(is_online);
                        last_ac_time = Some(std::time::Instant::now());
                        info!("Instant AC online change: {}", is_online);
                        let cfg = if is_online {
                            &config.state.charging
                        } else {
                            &config.state.discharging
                        };
                        for cmd in &cfg.exec.commands {
                            info!("Executing command for AC state: {cmd}");
                            let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
                        }
                        let n_cfg = &cfg.notification;
                        if n_cfg.enable {
                            state_notification = Some(
                                Notification::new()
                                    .summary(&n_cfg.summary)
                                    .body(&generate_body(&upower, &n_cfg.body, Some(is_online)).await?)
                                    .icon(&n_cfg.icon)
                                    .timeout(parse_timeout(n_cfg.timeout))
                                    .urgency((&n_cfg.urgency).into())
                                    .show_async()
                                    .await?,
                            );
                        }
                    }
                }
            }
            Some(msg) = percentage_stream.next() => {
                if let Ok(pct_float) = msg.get().await {
                    let pct = pct_float.round() as u64;
                    if last_pct != Some(pct) {
                        last_pct = Some(pct);
                        info!("Percentage changed to: {}%", pct);
                        for threshold in &config.percentage_thresholds {
                            if threshold.percentage == pct {
                                for cmd in &threshold.exec.commands {
                                    info!("Executing command for threshold {}%: {cmd}", pct);
                                    let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
                                }
                                let n_cfg = &threshold.notification;
                                if n_cfg.enable {
                                    info!("Triggering notification for {}% threshold", pct);
                                    let _ = Notification::new()
                                        .summary(&n_cfg.summary)
                                        .body(&generate_body(&upower, &n_cfg.body, None).await?)
                                        .icon(&n_cfg.icon)
                                        .timeout(parse_timeout(n_cfg.timeout))
                                        .urgency((&n_cfg.urgency).into())
                                        .show_async()
                                        .await;
                                }
                            }
                        }
                    }
                }
            }

            Some(msg) = warning_stream.next() => {
                let event = msg.get().await?;
                info!("Received event: WarningLevel::{event:?}");
                let cfg = match event {
                    WarningLevel::Unknown => &config.warning_level.unknown,
                    WarningLevel::None => &config.warning_level.none,
                    WarningLevel::Discharging => &config.warning_level.discharging,
                    WarningLevel::Low => &config.warning_level.low,
                    WarningLevel::Critical => &config.warning_level.critical,
                    WarningLevel::Action => &config.warning_level.action,
                };
                for cmd in &cfg.exec.commands {
                    info!("Executing command: {cmd}");
                    let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
                }
                let n_cfg = &cfg.notification;
                if n_cfg.enable {
                    warning_notification = Some(
                        Notification::new()
                            .summary(&n_cfg.summary)
                            .body(&generate_body(&upower, &n_cfg.body, None).await?)
                            .icon(&n_cfg.icon)
                            .timeout(parse_timeout(n_cfg.timeout))
                            .urgency((&n_cfg.urgency).into())
                            .show_async()
                            .await?,
                    );
                }
            }

            Some(msg) = state_stream.next() => {
                let event = msg.get().await?;
                info!("Received event: State::{event:?}");

                // Ignore Charging/Discharging/Pending state events here, because ac_online_stream handles them instantly
                match event {
                    State::Charging | State::Discharging | State::PendingCharge | State::PendingDischarge => {
                        debug!("Skipping State::{event:?} (handled instantly by AC online stream)");
                    }
                    _ => {
                        let cfg = match event {
                            State::Unknown => &config.state.unknown,
                            State::Empty => &config.state.empty,
                            State::FullyCharged => &config.state.fully_charged,
                            _ => &config.state.unknown,
                        };
                        for cmd in &cfg.exec.commands {
                            info!("Executing command: {cmd}");
                            let _ = Command::new("sh").arg("-c").arg(cmd).spawn();
                        }
                        let n_cfg = &cfg.notification;
                        if n_cfg.enable {
                            state_notification = Some(
                                Notification::new()
                                    .summary(&n_cfg.summary)
                                    .body(&generate_body(&upower, &n_cfg.body, None).await?)
                                    .icon(&n_cfg.icon)
                                    .timeout(parse_timeout(n_cfg.timeout))
                                    .urgency((&n_cfg.urgency).into())
                                    .show_async()
                                    .await?,
                            );
                        }
                    }
                }
            }

            _ = tokio::signal::ctrl_c() => {
                info!("Exiting...");
                break;
            }
        }
    }

    Ok(())
}

async fn generate_body(
    device: &DeviceProxy<'_>,
    template: &str,
    forced_is_charging: Option<bool>,
) -> Result<String> {
    let percentage = device.percentage().await?;
    let mut result = template.replace("{percentage}", &percentage.to_string());

    if result.contains("{time}") {
        let is_charging = match forced_is_charging {
            Some(c) => c,
            None => matches!(device.state().await?, State::Charging),
        };

        let time_val = if is_charging {
            device.time_to_full().await.unwrap_or(0)
        } else {
            device.time_to_empty().await.unwrap_or(0)
        };

        if time_val > 0 {
            let formatted = format_duration(Duration::from_secs(time_val as u64));
            let time_str = if is_charging {
                format!("{} until full", formatted)
            } else {
                formatted
            };
            result = result.replace("{time}", &time_str);
        } else {
            result = result
                .replace(" (~{time})", "")
                .replace(" ({time})", "")
                .replace("~{time}", "")
                .replace("{time}", "");
        }
    }

    Ok(result)
}

fn format_duration(duration: Duration) -> String {
    let total_seconds = duration.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;

    let mut parts = Vec::new();

    if hours > 0 {
        let hour_str = if hours == 1 { "hour" } else { "hours" };
        parts.push(format!("{hours} {hour_str}"));
    }

    if minutes > 0 {
        let minute_str = if minutes == 1 { "minute" } else { "minutes" };
        parts.push(format!("{minutes} {minute_str}"));
    }

    if parts.is_empty() {
        parts.push("0 minutes".to_owned());
    }

    parts.join(", ")
}
