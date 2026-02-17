use crate::config::ConfigError;
use crate::sugarloaf::font::SugarloafFont;

#[derive(Clone, Copy, PartialEq)]
pub enum TerminalErrorLevel {
    Warning,
    Error,
}

#[derive(Clone)]
pub struct TerminalError {
    pub report: TerminalErrorType,
    pub level: TerminalErrorLevel,
}

impl TerminalError {
    pub fn configuration_not_found() -> Self {
        TerminalError {
            level: TerminalErrorLevel::Warning,
            report: TerminalErrorType::ConfigurationNotFound,
        }
    }
}

impl From<ConfigError> for TerminalError {
    fn from(error: ConfigError) -> Self {
        match error {
            ConfigError::ErrLoadingConfig(message) => TerminalError {
                report: TerminalErrorType::InvalidConfigurationFormat(message),
                level: TerminalErrorLevel::Warning,
            },
            ConfigError::ErrLoadingTheme(message) => TerminalError {
                report: TerminalErrorType::InvalidConfigurationTheme(message),
                level: TerminalErrorLevel::Warning,
            },
            ConfigError::PathNotFound => TerminalError {
                report: TerminalErrorType::ConfigurationNotFound,
                level: TerminalErrorLevel::Warning,
            },
        }
    }
}

#[derive(Clone, PartialEq)]
pub enum TerminalErrorType {
    // font was not found
    FontsNotFound(Vec<SugarloafFont>),

    // navigation configuration has changed
    // NavigationHasChanged,
    InitializationError(String),

    // configurlation file was not found
    ConfigurationNotFound,
    // configuration file have an invalid format
    InvalidConfigurationFormat(String),
    // configuration invalid theme
    InvalidConfigurationTheme(String),

    // reports that are ignored by TerminalErrorType
    IgnoredReport,
}

impl std::fmt::Display for TerminalErrorType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            TerminalErrorType::FontsNotFound(fonts) => {
                let mut font_str = String::from("");
                for font in fonts.iter() {
                    let weight = if font.weight.is_none() {
                        String::from("any weight")
                    } else {
                        format!("{} weight", font.weight.unwrap())
                    };

                    let style = format!("{:?} style", font.style);

                    font_str +=
                        format!("\n• \"{}\" using {:?} {:?}", font.family, weight, style)
                            .as_str();
                }

                write!(f, "Font(s) not found:\n{font_str}")
            }
            TerminalErrorType::ConfigurationNotFound => {
                write!(f, "Configuration file was not found")
            }
            TerminalErrorType::InitializationError(message) => {
                write!(f, "Error initializing Omni Terminal:\n{message}")
            }
            TerminalErrorType::IgnoredReport => write!(f, ""),
            TerminalErrorType::InvalidConfigurationFormat(message) => {
                write!(f, "Found an issue loading the configuration file:\n\n{message}\n\nOmni Terminal will proceed with the default configuration\nhttps://terminal.omni.dev")
            }
            TerminalErrorType::InvalidConfigurationTheme(message) => {
                write!(f, "Found an issue in the configured theme:\n\n{message}")
            }
        }
    }
}
