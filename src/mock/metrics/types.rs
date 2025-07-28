//! Common types used across metrics modules

#[derive(Clone, Debug, PartialEq)]
pub enum PlatformType {
    Nvidia,
    Apple,
    Jetson,
    Intel,
    Amd,
    Tenstorrent,
    Rebellions,
    Furiosa,
}

impl PlatformType {
    pub fn from_str(platform_str: &str) -> Self {
        match platform_str.to_lowercase().as_str() {
            "nvidia" => PlatformType::Nvidia,
            "apple" => PlatformType::Apple,
            "jetson" => PlatformType::Jetson,
            "intel" => PlatformType::Intel,
            "amd" => PlatformType::Amd,
            "tenstorrent" | "tt" => PlatformType::Tenstorrent,
            "rebellions" | "rbln" => PlatformType::Rebellions,
            "furiosa" => PlatformType::Furiosa,
            _ => {
                eprintln!("Unknown platform '{platform_str}', defaulting to nvidia");
                PlatformType::Nvidia
            }
        }
    }
}
