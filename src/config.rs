use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Debug)]
pub struct Config {
    pub device: String,
    pub warning_level: WarningLevelConfig,
    pub state: StateConfig,
    #[serde(default)]
    pub percentage_thresholds: Vec<PercentageThresholdConfig>,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct PercentageThresholdConfig {
    pub percentage: u64,
    pub notification: NotificationConfig,
    #[serde(default)]
    pub exec: ExecConfig,
}

#[derive(Deserialize, Serialize, Debug)]
pub struct WarningLevelConfig {
    pub unknown: EventConfig,
    pub none: EventConfig,
    pub discharging: EventConfig,
    pub low: EventConfig,
    pub critical: EventConfig,
    pub action: EventConfig,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct StateConfig {
    pub unknown: EventConfig,
    pub charging: EventConfig,
    pub discharging: EventConfig,
    pub empty: EventConfig,
    pub fully_charged: EventConfig,
    pub pending_charge: EventConfig,
    pub pending_discharge: EventConfig,
}

#[derive(Deserialize, Serialize, Debug, Default)]
pub struct EventConfig {
    pub notification: NotificationConfig,
    pub exec: ExecConfig,
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct NotificationConfig {
    pub enable: bool,
    pub summary: String,
    pub body: String,
    pub icon: String,
    pub timeout: u32,
    pub urgency: UrgencyConfig,
}

#[derive(Deserialize, Serialize, Debug, Default, Clone)]
pub struct ExecConfig {
    pub commands: Vec<String>,
}

#[derive(Deserialize, Serialize, Debug, Clone, Default)]
#[serde(rename_all = "lowercase")]
pub enum UrgencyConfig {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Default for NotificationConfig {
    fn default() -> Self {
        Self {
            enable: false,
            summary: "Notification Summary".to_owned(),
            body: "Notification Body".to_owned(),
            icon: String::new(),
            timeout: 30000,
            urgency: UrgencyConfig::Normal,
        }
    }
}

impl From<&UrgencyConfig> for notify_rust::Urgency {
    fn from(val: &UrgencyConfig) -> Self {
        match val {
            UrgencyConfig::Low => Self::Low,
            UrgencyConfig::Normal => Self::Normal,
            UrgencyConfig::Critical => Self::Critical,
        }
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            device: "/org/freedesktop/UPower/devices/battery_BAT0".to_owned(),
            warning_level: WarningLevelConfig {
                none: EventConfig::default(),
                unknown: EventConfig::default(),
                discharging: EventConfig::default(),
                low: EventConfig {
                    exec: ExecConfig::default(),
                    notification: NotificationConfig {
                        enable: true,
                        summary: "Battery low".into(),
                        body: "Approximately <b>{time}</b> remaining ({percentage}%)".into(),
                        icon: "battery-low-symbolic".into(),
                        timeout: 30000,
                        urgency: UrgencyConfig::Normal,
                    },
                },
                critical: EventConfig {
                    exec: ExecConfig::default(),
                    notification: NotificationConfig {
                        enable: true,
                        summary: "Battery critically low".into(),
                        body: "Shutting down soon unless plugged in.".into(),
                        icon: "battery-caution-symbolic".into(),
                        timeout: 0,
                        urgency: UrgencyConfig::Critical,
                    },
                },
                action: EventConfig {
                    exec: ExecConfig::default(),
                    notification: NotificationConfig {
                        enable: true,
                        summary: "Battery critically low".into(),
                        body: "The battery is below the critical level and this computer is about to shutdown.".into(),
                        icon: "battery-action-symbolic".into(),
                        timeout: 0,
                        urgency: UrgencyConfig::Critical,
                    },
                },
            },
            state: StateConfig::default(),
            percentage_thresholds: Vec::new(),
        }
    }
}
