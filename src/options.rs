use clap::Parser;

#[derive(Parser, Debug)]
pub struct Options {
    /// Path of the config file
    #[clap(long, env = "CONFIG_FILE", default_value = "config.toml")]
    pub config: String,
}
